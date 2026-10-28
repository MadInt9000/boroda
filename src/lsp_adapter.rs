use crate::editor::LspDiagnostic;
use crate::lsp::LspClient;
use crate::tokens::RenderToken;
use crate::python_tokenizer::PythonTokenizer;
use crate::loader::ProjectLoader;
use lsp_types::Url;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub struct LspAdapter {
    pub version_counter: Arc<AtomicI64>,
    pub abs_filename_str: String,
    pub last_tokens_request_id: AtomicI64, 
    pub pending_definition_request_id: AtomicI64,
}

pub enum LspParseResult {
    Tokens(Vec<Vec<RenderToken>>),
    Diagnostics {
        errors: usize,
        warnings: usize,
        diagnostics: Vec<LspDiagnostic>,
    },    
    // НОВОЕ: Передает путь к файлу, целевую строку и колонку
    Definition {
        abs_path: String,
        line: usize,
        col: usize,
    },
}

impl LspAdapter {
    fn next_global_id() -> i64 {
        static GLOBAL_REQUEST_ID: AtomicI64 = AtomicI64::new(10);
        GLOBAL_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
    }

    pub fn new(abs_filename_str: String) -> Self {
        Self {
            version_counter: Arc::new(AtomicI64::new(1)),
            abs_filename_str,
            last_tokens_request_id: AtomicI64::new(0),
            pending_definition_request_id: AtomicI64::new(0),
        }
    }

    /// Уведомляет сервер об открытии текстового документа
    pub fn did_open(&self, lsp_pool: &[LspClient], text: &str) {
        if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
            if !client.is_started() { return; }

            let path = Path::new(&self.abs_filename_str);
            if !path.exists() { 
                return;
            }
            let file_url = Url::from_file_path(path).unwrap();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            
            let language_id = match ext {
                "py" => "python",
                "php" => "php",
                "rs" => "rust",
                _ => "todo",
            };

            let msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": file_url.to_string(),
                        "languageId": language_id,
                        "version": 1,
                        "text": text
                    }
                }
            });

            let json_str = serde_json::to_string(&msg).unwrap();
            client.send_raw_message(json_str);
        }
    }

    /// Отправляет на сервер измененное содержимое документа (Full Sync)
    pub fn send_change_notification(&self, lsp_pool: &[LspClient], text_content: &str) {
        if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
            if !client.is_started() { return; }

            let file_url = Url::from_file_path(Path::new(&self.abs_filename_str)).unwrap();
            let current_version = self.version_counter.fetch_add(1, Ordering::SeqCst) + 1;

            let msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {
                        "uri": file_url.to_string(),
                        "version": current_version
                    },
                    "contentChanges": [{ "text": text_content }]
                }
            });

            let json_str = serde_json::to_string(&msg).unwrap();
            client.send_raw_message(json_str);
        }
    }

    /// Отправляет текстовый запрос textDocument/definition на сервер
    pub fn request_definition(&self, lsp_pool: &[LspClient], line: usize, col: usize) {
        if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
            if !client.is_started() { return; }

            let next_id = Self::next_global_id();
            
            // НОВОЕ: Сразу сохраняем сгенерированный ID внутри адаптера
            self.pending_definition_request_id.store(next_id, Ordering::SeqCst);

            let file_url = Url::from_file_path(Path::new(&self.abs_filename_str)).unwrap();

            let msg = json!({
                "jsonrpc": "2.0",
                "id": next_id,
                "method": "textDocument/definition",
                "params": {
                    "textDocument": { "uri": file_url.to_string() },
                    "position": {
                        "line": line,
                        "character": col
                    }
                }
            });

            let json_str = serde_json::to_string(&msg).unwrap();
            client.send_raw_message(json_str);
        }
    }

    /// Запрашивает токены, самостоятельно определяя нужный LspClient по пулу
    pub fn request_syntax_tokens(&self, lsp_pool: &[LspClient], lines: &[Vec<char>]) -> Option<LspParseResult> {
        let ext = Path::new(&self.abs_filename_str)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if ext == "py" {
            let local_tokens = PythonTokenizer::tokenize_text(lines);
            Some(LspParseResult::Tokens(local_tokens))
        } else if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
            if client.is_started() {
                let next_id = Self::next_global_id();
                self.last_tokens_request_id.store(next_id, Ordering::SeqCst);
                
                client.request_semantic_tokens_with_id(&self.abs_filename_str, next_id);
            }
            None
        } else {
            None
        }
    }

    /// Отправляет серверу уведомление о физическом сохранении файла на диск
    pub fn send_save_notification(&self, lsp_pool: &[LspClient]) {
        if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
            if !client.is_started() { return; }

            let file_url = Url::from_file_path(Path::new(&self.abs_filename_str)).unwrap();

            let msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": {
                    "textDocument": {
                        "uri": file_url.to_string()
                    }
                }
            });

            let json_str = serde_json::to_string(&msg).unwrap();
            client.send_raw_message(json_str);
        }
    }

    /// Разбирает входящие сообщения JSON-RPC от пула серверов
    pub fn parse_lsp_message(
        &self, 
        lsp_pool: &[LspClient], 
        lsp_msg: &Value, 
        text_content: &str, 
        total_lines: usize   
    ) -> Option<LspParseResult> {
        
        if let Some(id) = lsp_msg.get("id").and_then(|id| id.as_i64()) {
            // id == 1: Фоновый хендшейк инициализации
            if id == 1 {
                if let Some(client) = ProjectLoader::get_server_client(&self.abs_filename_str, lsp_pool) {
                    if client.is_started() {
                        client.send_initialized();
                        self.did_open(lsp_pool, text_content);
                        
                        let next_id = Self::next_global_id();
                        self.last_tokens_request_id.store(next_id, Ordering::SeqCst);
                        
                        client.request_semantic_tokens_with_id(&self.abs_filename_str, next_id);
                    }
                }
                return None;
            }
            // НОВОЕ: Проверяем, является ли это ответом на наш pending_definition_request_id
            let expected_def_id = self.pending_definition_request_id.load(Ordering::SeqCst);
            if expected_def_id != 0 && id == expected_def_id {
                // Сбрасываем ID запроса, так как ответ получен
                self.pending_definition_request_id.store(0, Ordering::SeqCst);

                if let Some(result) = lsp_msg.get("result") {
                    let location = if result.is_array() {
                        result.get(0)
                    } else if result.is_object() {
                        Some(result)
                    } else {
                        None
                    };

                    if let Some(loc) = location {
                        if let (Some(uri_str), Some(range)) = (loc.get("uri").and_then(|u| u.as_str()), loc.get("range")) {
                            if let Ok(url) = Url::parse(uri_str) {
                                if let Ok(path) = url.to_file_path() {
                                    let abs_path = path.to_string_lossy().into_owned();
                                    let start = range.get("start");
                                    let line = Self::extract_pos(start, "line");
                                    let col = Self::extract_pos(start, "character");;

                                    return Some(LspParseResult::Definition { abs_path, line, col });
                                }
                            }
                        }
                    }
                }
            }
            // Обработка семантических токенов
            let expected_id = self.last_tokens_request_id.load(Ordering::SeqCst);
            if id == expected_id {
                if let Some(result) = lsp_msg.get("result") {
                    if let Some(data) = result.get("data").and_then(|d| d.as_array()) {
                        let mut new_tokens = vec![Vec::new(); total_lines];
                        let mut current_line = 0;
                        let mut current_char = 0;

                        for chunk in data.chunks_exact(5) {
                            let delta_line = chunk[0].as_u64().unwrap_or(0) as usize;
                            let delta_start = chunk[1].as_u64().unwrap_or(0) as usize;
                            let length = chunk[2].as_u64().unwrap_or(0) as usize;
                            let token_type = chunk[3].as_u64().unwrap_or(0) as u32;

                            if delta_line > 0 {
                                current_line += delta_line;
                                current_char = delta_start; 
                            } else {
                                current_char += delta_start;
                            }

                            if current_line < new_tokens.len() {
                                new_tokens[current_line].push(RenderToken {
                                    start_col: current_char,
                                    len: length,
                                    token_type,
                                });
                            }
                        }
                        return Some(LspParseResult::Tokens(new_tokens));
                    }
                }
            }
        }    
            
        // Обработка push-диагностики (publishDiagnostics)
        if lsp_msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
            if let Some(params) = lsp_msg.get("params") {
                if let (Some(uri_str), Ok(current_url)) = (
                    params.get("uri").and_then(|u| u.as_str()),
                    Url::from_file_path(Path::new(&self.abs_filename_str))
                ) {
                    if uri_str != current_url.as_str() { 
                        return None; 
                    }
                }

                let mut errors = 0;
                let mut warnings = 0;
                let mut diagnostics = Vec::new();

                if let Some(diagnostics_array) = params.get("diagnostics").and_then(|d| d.as_array()) {
                    for diag in diagnostics_array {
                        let severity = diag.get("severity").and_then(|s| s.as_i64()).unwrap_or(1);
                        if severity == 1 { errors += 1; } 
                        else if severity == 2 { warnings += 1; }

                        if let Some(range) = diag.get("range") {
                            diagnostics.push(LspDiagnostic {
                                start_line: Self::extract_pos(range.get("start"), "line"),
                                start_col: Self::extract_pos(range.get("start"), "character"),
                                end_line: Self::extract_pos(range.get("end"), "line"),
                                end_col:  Self::extract_pos(range.get("end"), "character"),
                                message: diag.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
                            });
                        }
                    }
                }
            return Some(LspParseResult::Diagnostics { errors, warnings, diagnostics });
            }
        }
    None
    }
    
    fn extract_pos(pos_obj: Option<&Value>, key: &str) -> usize {
        pos_obj
            .and_then(|p| p[key].as_u64())
            .unwrap_or(0) as usize
    }
}

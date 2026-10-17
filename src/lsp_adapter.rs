use std::collections::HashMap;
use crate::editor::LspDiagnostic;
use crate::lsp::LspClient;
use crate::tokens::RenderToken;
use crate::python_tokenizer::PythonTokenizer;

use lsp_types::Url;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub struct LspAdapter {
    pub version_counter: Arc<AtomicI64>,
    pub abs_filename_str: String,
}

// Чистый enum для передачи отпарсенных данных в главный цикл
pub enum LspParseResult {
    Tokens(Vec<Vec<RenderToken>>),
    Diagnostics {
        errors: usize,
        warnings: usize,
        diagnostics: Vec<LspDiagnostic>,
    },
}

impl LspAdapter {
    /// Создает экземпляр адаптера и канонизирует его абсолютный путь
    pub fn new(filename: &str) -> Self {
        let abs_path = Path::new(filename)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(filename).to_path_buf());
        let abs_filename_str = abs_path.to_string_lossy().into_owned();

        Self {
            version_counter: Arc::new(AtomicI64::new(1)),
            abs_filename_str,
        }
    }

    /// Уведомляет сервер об открытии текстового документа
    pub fn did_open(&self, client: &LspClient, text: &str) {
        let path = Path::new(&self.abs_filename_str);
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

    /// Отправляет на сервер измененное содержимое документа (Full Sync)
    pub fn send_change_notification(&self, client: &LspClient, text_content: &str) {
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

    /// Запрашивает токены. Если это Python — мгновенно возвращает локально распарсенную матрицу токенов.
    /// Если это другой язык — отправляет асинхронный запрос на сервер и возвращает None.
    pub fn request_syntax_tokens(&self, client: &LspClient, lines: &[Vec<char>]) -> Option<LspParseResult> {
        let ext = Path::new(&self.abs_filename_str)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if ext == "py" {
            // Если Python — генерируем матрицу токенов локально прямо из буфера строк
            let local_tokens = PythonTokenizer::tokenize_file(lines);
            Some(LspParseResult::Tokens(local_tokens))
        } else {
            // Для остальных языков запрашиваем асинхронно у внешнего LSP
            client.request_semantic_tokens(&self.abs_filename_str);
            None
        }
    }

    /// Отправляет серверу уведомление о физическом сохранении файла на диск
    pub fn send_save_notification(&self, client: &LspClient) {
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

    /// Разбирает сообщения от сервера. Принимает чистый текст и количество строк буфера.
    pub fn parse_lsp_message(
        &self, 
        lsp_pool: &HashMap<&'static str, LspClient>, 
        lsp_msg: &Value, 
        text_content: &str, // Передаем текст по ссылке
        total_lines: usize   // Передаем размер
    ) -> Option<LspParseResult> {
        
        //if let Ok(mut log_file) = std::fs::OpenOptions::new().create(true).append(true).open("lsp_debug.log") {
        //    use std::io::Write;
        //    let _ = writeln!(log_file, "SERVER: {}", lsp_msg);
       // }

        // 1. Проверяем ответы по ID запроса
        if let Some(id) = lsp_msg.get("id").and_then(|id| id.as_i64()) {
            
            // id == 1: Фоновый хендшейк инициализации
            if id == 1 {
                if let Some(key) = crate::loader::ProjectLoader::get_server_key(&self.abs_filename_str) {
                    if let Some(client) = lsp_pool.get(key) {
                        // Завершаем хендшейк
                        client.send_initialized();
                        
                        // Официально открываем документ на сервере, используя переданный текст
                        self.did_open(client, text_content);
                        
                        // Запрашиваем токены подсветки синтаксиса
                        client.request_semantic_tokens(&self.abs_filename_str);
                    }
                }
                return None;
            }

            // id == 2: Обработка пришедших семантических токенов
            if id == 2 {
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
                                current_char = 0;
                            }
                            current_char += delta_start;

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

        // 2. Обработка push-диагностики ошибок проекта (publishDiagnostics)
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
                                start_line: range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize,
                                start_col: range.get("start").and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize,
                                end_line: range.get("end").and_then(|e| e.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize,
                                end_col: range.get("end").and_then(|e| e.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize,
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
}

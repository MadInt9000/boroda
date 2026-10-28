use crate::editor::Editor;
use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;
use crate::python_tokenizer::PythonTokenizer;
use crate::markdown_tokenizer::MarkdownTokenizer;
use crate::config_tokenizer::ConfigTokenizer;
use crate::config::Configuration;
use std::env;
use std::path::Path;
use tokio::sync::mpsc;
use serde_json::Value;

pub struct ProjectLoader;

impl ProjectLoader {
    /// Возвращает прямую ссылку на LspClient из пула на основе расширения файла
    pub fn get_server_client<'a>(filename: &str, lsp_pool: &'a [LspClient]) -> Option<&'a LspClient> {
        let ext = Path::new(filename).extension().and_then(|s| s.to_str()).unwrap_or("");
        let config = Configuration::global();
        
        // Получаем строку-идентификатор (например, "python") для этого расширения
        if let Some(server_id) = config.file_types.get(ext) {
            // Ищем в пуле сервер, у которого совпадает server_id
            return lsp_pool.iter().find(|client| client.server_id == *server_id);
        }
        None
    }

    /// Динамическая проверка поддержки автозагрузки токенизаторов
    pub fn supported_autoload(ext: &str) -> bool {
        let config = Configuration::global();
        // Файл поддерживается, если для него зарегистрирован LSP-сервер или локальный парсер
        config.file_types.contains_key(ext) || matches!(ext, "md" | "json" | "toml" | "ini" | "conf")
    }

    /// ДИНАМИЧЕСКИЙ ПУЛ: Создает заглушки серверов строго на основе конфигурации JSON
    pub fn create_empty_pool() -> Vec<LspClient> {
        let config = Configuration::global();
        let mut pool = Vec::new();
        
        for server_id in config.lsp_servers.keys() {
            pool.push(LspClient::new_stub(server_id.clone()));
        }
        pool
    }

    // Метод снова СИНХРОННЫЙ
    pub fn load_single_file(
        raw_filename: String,
        lsp_pool: &[LspClient], // Передаем как обычный срез пула
        global_tx: mpsc::UnboundedSender<Value>,
        current_dir: &str,
    ) -> Editor {
        let path = Path::new(&raw_filename);
        //let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| {
            // 2. ФОЛЛБЭК: Если файла нет на диске, проверяем, абсолютный ли он изначально
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                // 3. Если путь относительный, вручную склеиваем его с текущей рабочей директорией
                let mut base = std::env::current_dir().unwrap_or_else(|_| Path::new(current_dir).to_path_buf());
                base.push(path);
                base
            }
        });
        let abs_filename_str = abs_path.to_string_lossy().into_owned();
        let mut editor = Editor::new(abs_filename_str);
        let ext = Path::new(&editor.filename).extension().and_then(|s| s.to_str()).unwrap_or("");
        
        // Получаем прямую ссылку на клиент вместо индекса
        if let Some(client) = Self::get_server_client(&editor.filename, lsp_pool) {
            let adapter = LspAdapter::new(editor.filename.clone());
            
            // Запускаем сервер напрямую через ссылку
            client.start(current_dir, global_tx);
            editor.lsp_adapter = Some(adapter);
            
            // Проверяем по строковому ID
            if client.server_id == "python" {
                let local_tokens = PythonTokenizer::tokenize_text(&editor.lines);
                editor.update_semantic_tokens(local_tokens);
            }
        } else if ext == "md" {
            // НОВОЕ: Если это Markdown-файл, запускаем наш локальный токенизатор
            let local_tokens = MarkdownTokenizer::tokenize_text(&editor.lines);
            editor.update_semantic_tokens(local_tokens); 
        } else if matches!(ext, "json" | "toml" | "ini" | "conf") {
            // НОВОЕ: Парсим конфигурационные файлы при первичной загрузке
            let local_tokens = ConfigTokenizer::tokenize_text(&editor.lines);
            editor.update_semantic_tokens(local_tokens);
        }


        let _ = editor.update_window_size();
        editor.scroll();
        editor
    }

    // Метод снова СИНХРОННЫЙ
    pub fn load_project(
        global_tx: mpsc::UnboundedSender<Value>,
        current_dir: &str,
    ) -> (Vec<Editor>, Vec<LspClient>) {
        let lsp_pool = Self::create_empty_pool();
        let mut filenames: Vec<String> = env::args().skip(1).collect();
        let config = Configuration::global();
        if filenames.is_empty() {
            if let Ok(entries) = std::fs::read_dir(".") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if config.file_types.contains_key(ext) || matches!(ext, "md" | "txt" | "toml" | "json" | "ini" | "conf") {
                                    filenames.push(path.to_string_lossy().into_owned());
                                }
                            }
                        }
                    }
                }
            }
        }

        if filenames.is_empty() {
            filenames.push("untitled.txt".to_string());
        }

        let mut editors = Vec::new();
        for filename in filenames {
            // Синхронный вызов без всяких .await
            let editor = Self::load_single_file(
                filename, 
                &lsp_pool, 
                global_tx.clone(), 
                current_dir
            );
            editors.push(editor);
        }

        (editors, lsp_pool)
    }
}


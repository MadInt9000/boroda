use crate::editor::Editor;
use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;

use std::collections::HashMap;
use std::env;
use std::path::Path;
use tokio::sync::mpsc;
use serde_json::Value;

pub struct ProjectLoader;

impl ProjectLoader {
    /// Вспомогательный метод для определения ключа сервера по расширению
    pub fn get_server_key(filename: &str) -> Option<&'static str> {
        let ext = Path::new(filename).extension().and_then(|s| s.to_str()).unwrap_or("");
        match ext {
            "py" => Some("py"),
            "rs" => Some("rs"),
            "php" => Some("php"),
            _ => None,
        }
    }

    /// Метод для полной загрузки ОДИНОЧНОГО файла. 
    /// Создает экземпляр Editor, при необходимости запускает LspClient и связывает их.
    pub fn load_single_file(
        filename: String,
        lsp_pool: &mut HashMap<&'static str, LspClient>,
        global_tx: mpsc::UnboundedSender<Value>,
        current_dir: &str,
    ) -> Editor {
        let mut editor = Editor::new(filename);
        
        if let Some(key) = Self::get_server_key(&editor.filename) {
            // Создаем чистый адаптер метаданных
            let mut adapter = LspAdapter::new(&editor.filename);

            // Ленивый запуск LspClient, если для этого языка сервер еще не поднят
            if !lsp_pool.contains_key(key) {
                let cmd = match key {
                    "py" => "pyright-langserver",
                    "rs" => "rust-analyzer",
                    "php" => "intelephense",
                    _ => "",
                };
                
                let (client, mut rx) = LspClient::start(cmd, current_dir);
                let tx = global_tx.clone();
                
                // Перенаправляем сообщения сервера в единый глобальный канал
                tokio::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        let _ = tx.send(msg);
                    }
                });
                client.send_initialize(current_dir);                
                lsp_pool.insert(key, client);
            }

            //editor.lsp_adapter = Some(adapter);
            // Извлекаем ссылку на клиент из пула для отправки первичных уведомлений
//            if let Some(client) = lsp_pool.get(key) {
//                let text_content = editor.get_all_text();
//                adapter.did_open(client, &text_content);
//                adapter.request_syntax_tokens(client, &editor.lines);
//            }

            editor.lsp_adapter = Some(adapter);
        }

        // Обновляем геометрию окна под терминал и скроллим
        let _ = editor.update_window_size();
        editor.scroll();
        
        editor
    }

    /// Главный метод загрузки проекта. 
    /// Сканирует окружение и делегирует создание каждого файла методу load_single_file.
    pub fn load_project(
        global_tx: mpsc::UnboundedSender<Value>,
        current_dir: &str,
    ) -> (Vec<Editor>, HashMap<&'static str, LspClient>) {
        let mut lsp_pool: HashMap<&'static str, LspClient> = HashMap::new();
        let mut filenames: Vec<String> = env::args().skip(1).collect();

        // Сканируем папку при отсутствии аргументов командной строки
        if filenames.is_empty() {
            if let Ok(entries) = std::fs::read_dir(".") {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            let path = entry.path();
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if matches!(ext, "py" | "rs" | "php") {
                                    filenames.push(path.to_string_lossy().into_owned());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Фоллбэк для пустого проекта
        if filenames.is_empty() {
            filenames.push("untitled.txt".to_string());
        }

        // Вызываем метод загрузки единичного файла для каждого элемента в цикле
        let mut editors = Vec::new();
        for filename in filenames {
            let editor = Self::load_single_file(
                filename, 
                &mut lsp_pool, 
                global_tx.clone(), 
                current_dir
            );
            editors.push(editor);
        }

        (editors, lsp_pool)
    }
}

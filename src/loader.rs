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
    /// Теперь сам отвечает за приведение любого пути к абсолютному каноническому виду.
    pub fn load_single_file(
        raw_filename: String, // Принимает любой сырой путь (относительный или абсолютный)
        lsp_pool: &mut HashMap<&'static str, LspClient>,
        global_tx: mpsc::UnboundedSender<Value>,
        current_dir: &str,
    ) -> Editor {
        // КАНОНИЗАЦИЯ: Превращаем сырой путь в абсолютный на самом раннем этапе внутри метода
        let path = Path::new(&raw_filename);
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let abs_filename_str = abs_path.to_string_lossy().into_owned();

        // Создаем Editor, строго используя абсолютный путь
        let mut editor = Editor::new(abs_filename_str);
        
        if let Some(key) = Self::get_server_key(&editor.filename) {
            // Создаем адаптер метаданных и передаем ему каноничный путь
            let adapter = LspAdapter::new(editor.filename.clone());

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

            editor.lsp_adapter = Some(adapter);
        }

        // Обновляем геометрию окна под терминал и скроллим
        let _ = editor.update_window_size();
        editor.scroll();
        
        editor
    }

    /// Главный метод загрузки проекта. 
    /// Теперь просто собирает сырые имена файлов и безопасно делегирует их в load_single_file.
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
                                    // Передаем как сырую строку, load_single_file сам разберется с путем
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

use crate::editor::Editor;
use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;
use std::env;
use std::path::Path;
use tokio::sync::mpsc;
use serde_json::Value;

pub struct ProjectLoader;

impl ProjectLoader {
    pub fn get_server_index(filename: &str) -> Option<usize> {
        let ext = Path::new(filename).extension().and_then(|s| s.to_str()).unwrap_or("");
        match ext {
            "py" => Some(0),
            "rs" => Some(1),
            "php" => Some(2),
            _ => None,
        }
    }

    pub fn create_empty_pool() -> Vec<LspClient> {
        vec![
            LspClient::new_stub("pyright-langserver"), 
            LspClient::new_stub("rust-analyzer"),      
            LspClient::new_stub("intelephense"),       
        ]
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
        
        if let Some(idx) = Self::get_server_index(&editor.filename) {
            let adapter = LspAdapter::new(editor.filename.clone());

            // СИНХРОННЫЙ СТАРТ: метод сам проверит флаг is_started внутри мьютекса
            lsp_pool[idx].start(current_dir, global_tx);

            editor.lsp_adapter = Some(adapter);
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


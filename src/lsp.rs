use lsp_types::Url;
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Mutex; // Используем СИНХРОННЫЙ мьютекс для внутренней мутабельности
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct LspClient {
    // Храним идентификатор сервера из конфига (например, "python", "rust")
    pub server_id: String,
    // Внутреннее состояние скрыто под синхронным Mutex
    inner: Mutex<LspClientState>,
}

struct LspClientState {
    is_started: bool,
    tx: Option<mpsc::UnboundedSender<String>>,
    _process: Option<Child>,
}

impl LspClient {
    /// Создает преднастроенный клиент на основе имени из конфига
    pub fn new_stub(server_id: String) -> Self {
        Self {
            server_id,
            inner: Mutex::new(LspClientState {
                is_started: false,
                tx: None,
                _process: None,
            }),
        }
    }

    /// Публичный метод для проверки флага запущенности
    pub fn is_started(&self) -> bool {
        self.inner.lock().unwrap().is_started
    }

    /// Полностью СИНХРОННЫЙ метод старта. Просто проверяет флаг и запускает процесс.
    pub fn start(&self, current_dir: &str, global_tx: mpsc::UnboundedSender<Value>) {
        let mut state = self.inner.lock().unwrap();
        
        // 1. Проверяем флаг — если уже запущен, ничего не делаем
        if state.is_started {
            return;
        }

        // 2. Синхронно настраиваем и запускаем процесс через Command::spawn
        // Извлекаем настройки конкретного сервера из нашей конфигурации
        let config = crate::config::Configuration::global();
        let server_config = match config.lsp_servers.get(&self.server_id) {
            Some(cfg) => cfg,
            None => {
                // Если настроек для сервера нет, не запускаем процесс
                return;
            }
        };

        let mut cmd = Command::new(&server_config.command);
        // Добавляем кастомные аргументы из JSON (например, ["--stdio"])
        cmd.args(&server_config.args);

        let mut child = cmd
            .current_dir(current_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start LSP server process");

        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");

        let (tx_in, mut rx_in) = mpsc::unbounded_channel::<String>();
        let (tx_out, mut rx_out) = mpsc::unbounded_channel::<Value>();

        // 3. Поток записи на сервер (асинхронный фон)
        tokio::spawn(async move {
            while let Some(msg) = rx_in.recv().await {
                let full_msg = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
                if stdin.write_all(full_msg.as_bytes()).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // 4. Перенаправление ответов в единый глобальный канал приложения
        let tx_global = global_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx_out.recv().await {
                let _ = tx_global.send(msg);
            }
        });

        // 5. Поток чтения ответов от сервера (асинхронный фон)
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                let mut content_length = 0;
                loop {
                    line.clear();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let trimmed = line.trim_matches(|c| c == '\r' || c == '\n');
                    if trimmed.is_empty() {
                        break;
                    }
                    if trimmed.starts_with("Content-Length:") {
                        if let Some(len_str) = trimmed.split(':').nth(1) {
                            content_length = len_str.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }

                if content_length == 0 { return; }

                let mut buf = vec![0u8; content_length];
                if reader.read_exact(&mut buf).await.is_ok() {
                    if let Ok(json_val) = serde_json::from_slice::<Value>(&buf) {
                        if tx_out.send(json_val).is_err() {
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
        });

        // Запоминаем канал отправки и дочерний процесс
        state.tx = Some(tx_in);
        state._process = Some(child);
        state.is_started = true;

        // Инициируем фоновое фоновое приветствие (отправку initialize осуществляем сразу)
        // Так как send_initialize теперь синхронный, вызываем его прямо здесь
        drop(state); // Освобождаем lock перед отправкой сообщений самой себе
        self.send_initialize(current_dir);
    }

    pub fn send_raw_message(&self, json_str: String) {
        if let Some(ref tx) = self.inner.lock().unwrap().tx {
            let _ = tx.send(json_str);
        }
    }

    pub fn send_initialize(&self, current_dir: &str) {
        let path = Path::new(current_dir);
        let root_url = Url::from_directory_path(path).unwrap();
        
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_url.to_string(),
                "rootPath": current_dir,
                "workspaceFolders": [{"uri": root_url.to_string(), "name": "root"}],
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "change": 1,
                            "dynamicRegistration": true,
                            "willSave": false,
                            "willSaveWaitUntil": false,
                            "didSave": true
                        }
                    }
                }
            }
        });

        let json_str = serde_json::to_string(&msg).unwrap();
        self.send_raw_message(json_str);
    }

    pub fn send_initialized(&self) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        self.send_raw_message(json_str);
    }

    pub fn request_semantic_tokens_with_id(&self, filename: &str, request_id: i64) {
        let abs_path = std::fs::canonicalize(filename).unwrap_or_else(|_| Path::new(filename).to_path_buf());
        let file_url = Url::from_file_path(&abs_path).unwrap();

        let msg = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "textDocument/semanticTokens/full",
            "params": {
                "textDocument": { "uri": file_url.to_string() }
            }
        });

        let json_str = serde_json::to_string(&msg).unwrap();
        self.send_raw_message(json_str);
    }
}


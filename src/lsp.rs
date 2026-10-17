use lsp_types::Url;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct LspClient {
    _process: Child,
    tx: mpsc::UnboundedSender<String>,
}

impl LspClient {
    pub fn start(server_command: &str, current_dir: &str) -> (Self, mpsc::UnboundedReceiver<Value>) {
        let mut cmd = Command::new(server_command);
        if server_command.contains("pyright") || server_command.contains("intelephense") {
            cmd.arg("--stdio");
        }

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
        let (tx_out, rx_out) = mpsc::unbounded_channel::<Value>();

        // 1. Поток записи на сервер
        tokio::spawn(async move {
            while let Some(msg) = rx_in.recv().await {
                let full_msg = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
                if stdin.write_all(full_msg.as_bytes()).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // 2. Поток чтения ответов от сервера
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

                if content_length == 0 {
                    return;
                }

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

        (Self { _process: child, tx: tx_in }, rx_out)
    }

    pub fn send_raw_message(&self, json_str: String) {
        let _ = self.tx.send(json_str);
    }

    pub fn send_initialize(&self, current_dir: &str) {
        let path = std::path::Path::new(current_dir);
        let root_url = Url::from_directory_path(path).unwrap();
        
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_url.to_string(),
                "rootPath": current_dir,
                "workspaceFolders": [
                    {
                        "uri": root_url.to_string(),
                        "name": "root"
                    }
                ],
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "change": 1, // Full Sync
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

    pub fn request_semantic_tokens(&self, filename: &str) {
        let abs_path = std::fs::canonicalize(filename)
            .unwrap_or_else(|_| std::path::Path::new(filename).to_path_buf());
        let file_url = Url::from_file_path(&abs_path).unwrap();

        let msg = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/semanticTokens/full",
            "params": {
                "textDocument": { "uri": file_url.to_string() }
            }
        });

        let json_str = serde_json::to_string(&msg).unwrap();
        self.send_raw_message(json_str);
    }
}


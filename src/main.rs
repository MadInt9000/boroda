mod editor;
mod tokens;
mod input;
mod lsp;
mod lsp_adapter;
mod python_tokenizer;
mod loader;

use input::InputDriver;
use loader::ProjectLoader;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use serde_json::Value;
use std::env;
use std::io;
use std::panic;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::lsp_adapter::LspParseResult;

#[tokio::main]
async fn main() -> io::Result<()> {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
        let _ = terminal::disable_raw_mode();
        default_hook(panic_info);
    }));

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let current_dir = env::current_dir()?.to_string_lossy().into_owned();
    let (global_tx, mut global_rx) = mpsc::unbounded_channel::<Value>();

    // --- Вся логика инициализации ушла в ProjectLoader ---
    let (mut editors, lsp_pool) = ProjectLoader::load_project(global_tx, &current_dir);

    let mut current_editor_idx = 0;
    let mut reader = EventStream::new();
    let mut has_pending_changes = false;

    editors[current_editor_idx].draw(&mut stdout)?;

    // ЧИСТЫЙ ГЛАВНЫЙ ЦИКЛ (Event Loop)
    while !editors[current_editor_idx].should_quit {
        let timeout_duration = if has_pending_changes {
            Duration::from_millis(700)
        } else {
            Duration::from_secs(3600 * 24)
        };

        tokio::select! {
            // Ветка 1: Ввод пользователя (передаем ссылку на lsp_pool)
            event_option = reader.next() => {
                if let Some(Ok(event)) = event_option {
                    InputDriver::handle_input(&mut editors, &mut current_editor_idx, &lsp_pool, event).await?;
                    if editors[current_editor_idx].should_quit { break; }

                    editors[current_editor_idx].update_window_size()?;
                    editors[current_editor_idx].scroll();
                    editors[current_editor_idx].draw(&mut stdout)?;
                    has_pending_changes = true;
                }
            }

            // Ветка 2: Таймер затишья (Debounce)
            _ = tokio::time::sleep(timeout_duration), if has_pending_changes => {
                let active_editor = &mut editors[current_editor_idx];
                
                if let (Some(ref lsp), Some(key)) = (&active_editor.lsp_adapter, ProjectLoader::get_server_key(&active_editor.filename)) {
                    if let Some(client) = lsp_pool.get(key) {
                        let text_content = active_editor.get_all_text();
                        
                        lsp.send_change_notification(client, &text_content);

                        // Вызываем метод и передаем срез строк буфера. 
                        // Если это Python — мы сразу получим и применим локальные токены.
                        if let Some(LspParseResult::Tokens(local_tokens)) = lsp.request_syntax_tokens(client, &active_editor.lines) {
                            active_editor.update_semantic_tokens(local_tokens);
                        }
                    }
                }
                has_pending_changes = false;
            }

            // Ветка 3: Ответы от серверов в src/main.rs
            lsp_msg_option = global_rx.recv() => {
                if let Some(lsp_msg) = lsp_msg_option {
                    let mut state_changed = false;
                    
                    for editor in &mut editors {
                        // 1. Предварительно собираем атомарные данные из editor
                        let text_content = editor.get_all_text();
                        let total_lines = editor.lines.len();

                        // 2. Спокойно вызываем парсинг — никакого конфликта заимствований больше нет!
                        let parse_result = if let Some(ref lsp) = editor.lsp_adapter {
                            lsp.parse_lsp_message(&lsp_pool, &lsp_msg, &text_content, total_lines)
                        } else {
                            None
                        };

                        if let Some(result) = parse_result {
                            match result {
                                LspParseResult::Tokens(ready_tokens) => {
                                    editor.update_semantic_tokens(ready_tokens);
                                    state_changed = true;
                                }
                                LspParseResult::Diagnostics { errors, warnings, diagnostics } => {
                                    editor.error_count = errors;
                                    editor.warning_count = warnings;
                                    editor.diagnostics = diagnostics;
                                    state_changed = true;
                                }
                            }
                        }
                    }

                    if state_changed {
                        editors[current_editor_idx].update_window_size()?;
                        editors[current_editor_idx].scroll();
                        editors[current_editor_idx].draw(&mut stdout)?;
                    }
                }
            }
        }
    }

    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

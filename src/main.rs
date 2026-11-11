mod base_editor;
mod clipboard;
mod config;
mod config_tokenizer;
mod editor;
mod history;
mod input;
mod intermediate;
mod loader;
mod lsp;
mod lsp_adapter;
mod markdown_tokenizer;
mod modal;
mod python_tokenizer;
mod rx_worker;
mod tokens;
mod traits;
use crate::lsp_adapter::LspParseResult;
use config_tokenizer::ConfigTokenizer;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor::Editor;
use futures_util::StreamExt;
use intermediate::IntermediateState;
use loader::ProjectLoader;
use markdown_tokenizer::MarkdownTokenizer;
use rx_worker::RxWorker;
use serde_json::Value;
use std::env;
use std::io;
use std::panic;
use std::time::Duration;
use tokio::sync::mpsc;

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
    let (mut editors, lsp_pool) = ProjectLoader::load_project(global_tx.clone(), &current_dir);

    let mut current_editor_idx = 0;
    let mut reader = EventStream::new();
    let mut has_pending_changes = false;

    // Промежуточный слой сам открывает модалки и применяет их результаты.
    let mut intermediate = IntermediateState::new();

    editors[current_editor_idx].base.draw(&mut stdout)?;

    // ЧИСТЫЙ ГЛАВНЫЙ ЦИКЛ (Event Loop)
    'outer: loop {
        // Представления собираются один раз на входе и пересобираются
        // только если набор редакторов изменился (например, открыт новый файл).
        let (mut text_editors, mut file_editors) = Editor::split_views(&mut editors);

        while !text_editors[current_editor_idx].should_quit() {
            let timeout_duration = if has_pending_changes {
                Duration::from_millis(700)
            } else {
                Duration::from_secs(3600 * 24)
            };

            let mut new_file: Option<(String, usize, usize)> = None;

            tokio::select! {
                // Ветка 1: Ввод пользователя (передаем ссылку на lsp_pool)
                event_option = reader.next() => {
                    if let Some(Ok(event)) = event_option {
                        has_pending_changes = intermediate.handle_event(
                            &mut text_editors,
                            &mut file_editors,
                            &mut current_editor_idx,
                            &lsp_pool,
                            event,
                            &mut stdout,
                        ).await?;
                    }
                }

                // Ветка 2: В цикле Debounce
                _ = tokio::time::sleep(timeout_duration), if has_pending_changes && !intermediate.is_modal_active() => {
                    let active_text = &mut *text_editors[current_editor_idx];
                    let ext = std::path::Path::new(file_editors[current_editor_idx].filename())
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");

                    if let Some(lsp) = file_editors[current_editor_idx].lsp_adapter() {
                        let text_content = active_text.get_all_text();

                        // Всё вызывается синхронно!
                        lsp.send_change_notification(&lsp_pool, &text_content);

                        if let Some(LspParseResult::Tokens(local_tokens)) =
                            lsp.request_syntax_tokens(&lsp_pool, active_text.lines())
                        {
                            active_text.update_semantic_tokens(local_tokens);
                        }
                    } else if ext == "md" {
                        // НОВОЕ: Пересчитываем локальные Markdown токены при изменении текста
                        let local_tokens = MarkdownTokenizer::tokenize_text(active_text.lines());
                        active_text.update_semantic_tokens(local_tokens);
                    } else if matches!(ext, "json" | "toml" | "ini" | "conf") {
                        // НОВОЕ: Пересчитываем токены конфигурации при редактировании
                        let local_tokens = ConfigTokenizer::tokenize_text(active_text.lines());
                        active_text.update_semantic_tokens(local_tokens);
                    }
                    has_pending_changes = false;
                }

                // Ветка 3: Ответы от серверов (обрабатываются в rx_worker)
                lsp_msg_option = global_rx.recv() => {
                    if let Some(lsp_msg) = lsp_msg_option {
                        new_file = RxWorker::handle_message(
                            &lsp_msg,
                            &mut text_editors,
                            &mut file_editors,
                            &mut current_editor_idx,
                            &lsp_pool,
                            &mut stdout,
                            !intermediate.is_modal_active()
                        )?;
                    }
                }
            }

            // Определение указывает на неоткрытый файл: освобождаем представления,
            // добавляем редактор и заходим в цикл заново с новыми векторами.
            if let Some((target_path, line, col)) = new_file {
                drop(text_editors);
                drop(file_editors);

                let new_editor = ProjectLoader::load_single_file(
                    target_path,
                    &lsp_pool,
                    global_tx.clone(),
                    &current_dir,
                );
                editors.push(new_editor);
                current_editor_idx = editors.len() - 1;

                editors[current_editor_idx].base.set_cursor(line, col);
                editors[current_editor_idx].base.update_window_size()?;
                editors[current_editor_idx].base.scroll();
                editors[current_editor_idx].base.draw(&mut stdout)?;

                has_pending_changes = true;
                continue 'outer;
            }
        }

        break;
    }

    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

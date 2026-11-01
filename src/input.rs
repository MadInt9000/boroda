use crate::lsp::LspClient;
use crate::traits::{FileOperations, TextInteractive};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::io;

pub struct InputDriver;

impl InputDriver {
    pub async fn handle_input(
        text_editors: &mut Vec<&mut dyn TextInteractive>,
        file_editors: &mut Vec<&mut dyn FileOperations>,
        current_idx: &mut usize,
        lsp_pool: &[LspClient],
        event: Event,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        Self::process_event(text_editors, file_editors, current_idx, lsp_pool, event).await?;

        // Отображение — тоже часть интерактивного текстового слоя
        let editor = &mut *text_editors[*current_idx];
        editor.update_window_size()?;
        editor.scroll();
        editor.draw(stdout)?;

        Ok(())
    }

    async fn process_event(
        text_editors: &mut Vec<&mut dyn TextInteractive>,
        file_editors: &mut Vec<&mut dyn FileOperations>,
        current_idx: &mut usize,
        lsp_pool: &[LspClient],
        event: Event,
    ) -> io::Result<()> {
        match event {
            Event::Key(key) => {
                let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);
                let has_control = key.modifiers.contains(KeyModifiers::CONTROL);

                // Управление выделением при зажатом Shift
                if matches!(
                    key.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
                ) {
                    if has_shift {
                        text_editors[*current_idx].begin_selection();
                    } else {
                        text_editors[*current_idx].clear_selection();
                    }
                }

                // CTRL-комбинации
                if has_control {
                    match key.code {
                        KeyCode::PageUp => {
                            Self::previous_editor(text_editors.len(), current_idx);
                            return Ok(());
                        }
                        KeyCode::PageDown => {
                            Self::next_editor(text_editors.len(), current_idx);
                            return Ok(());
                        }
                        KeyCode::Left => {
                            text_editors[*current_idx].move_word_left();
                            return Ok(());
                        }
                        KeyCode::Right => {
                            text_editors[*current_idx].move_word_right();
                            return Ok(());
                        }
                        KeyCode::Insert => {
                            text_editors[*current_idx].paste_from_clipboard();
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // Файловые операции отправляются во второй вектор
                match key.code {
                    KeyCode::F(2) => {
                        Self::save(text_editors, file_editors, *current_idx, lsp_pool)?;
                        return Ok(());
                    }
                    KeyCode::F(4) => {
                        let line = text_editors[*current_idx].cursor_y();
                        let col = text_editors[*current_idx].cursor_x();
                        if let Some(lsp) = file_editors[*current_idx].lsp_adapter() {
                            lsp.request_definition(lsp_pool, line, col);
                        }
                        return Ok(());
                    }
                    KeyCode::F(11) => {
                        Self::save(text_editors, file_editors, *current_idx, lsp_pool)?;
                        text_editors[*current_idx].quit();
                        return Ok(());
                    }
                    KeyCode::F(12) => {
                        let text = &mut *text_editors[*current_idx];
                        let file = &mut *file_editors[*current_idx];
                        file.reload_file(text);
                        return Ok(());
                    }
                    _ => {}
                }

                // Текстовые операции и навигация
                let editor = &mut *text_editors[*current_idx];
                match key.code {
                    KeyCode::F(1) => editor.toggle_indent(),
                    KeyCode::F(3) => editor.search_substring().await,
                    KeyCode::F(5) => editor.copy_to_clipboard(),
                    KeyCode::F(6) => editor.paste_from_clipboard(),
                    KeyCode::F(7) => editor.clear_current_line(),
                    KeyCode::F(8) => editor.cut_to_clipboard(),
                    KeyCode::F(9) => editor.move_to_line_start(),
                    KeyCode::F(10) => editor.quit(),

                    KeyCode::PageUp => editor.page_up(),
                    KeyCode::PageDown => editor.page_down(),
                    KeyCode::Home => editor.move_to_line_start(),
                    KeyCode::End => editor.move_to_line_end(),
                    KeyCode::Up => editor.move_up(),
                    KeyCode::Down => editor.move_down(),
                    KeyCode::Left => editor.move_left(),
                    KeyCode::Right => editor.move_right(),

                    KeyCode::Delete => {
                        if editor.has_selection() {
                            editor.delete_selection();
                        } else {
                            editor.delete_char_at_cursor();
                        }
                    }
                    KeyCode::Tab => {
                        if editor.has_selection() {
                            editor.delete_selection();
                        }
                        editor.insert_str("    ");
                    }
                    KeyCode::Char(c) => {
                        if editor.has_selection() {
                            editor.delete_selection();
                        }
                        editor.insert_char(c);
                    }
                    KeyCode::Backspace => {
                        if editor.has_selection() {
                            editor.delete_selection();
                        } else {
                            editor.delete_char();
                        }
                    }
                    KeyCode::Enter => {
                        if editor.has_selection() {
                            editor.delete_selection();
                        }
                        editor.insert_newline();
                    }
                    _ => {}
                }
            }

            Event::Mouse(mouse_event) => {
                let has_shift = mouse_event.modifiers.contains(KeyModifiers::SHIFT);
                let has_control = mouse_event.modifiers.contains(KeyModifiers::CONTROL);

                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if has_shift {
                            text_editors[*current_idx].begin_selection();
                        } else {
                            text_editors[*current_idx].clear_selection();
                        }
                        text_editors[*current_idx]
                            .click_mouse(mouse_event.column, mouse_event.row);
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if mouse_event.row == 0 {
                            Self::previous_editor(text_editors.len(), current_idx);
                        } else if has_control {
                            let line = text_editors[*current_idx].row_offset()
                                + mouse_event.row as usize
                                - 1;
                            let col = text_editors[*current_idx].col_offset()
                                + mouse_event.column as usize;

                            if let Some(lsp) = file_editors[*current_idx].lsp_adapter() {
                                lsp.request_definition(lsp_pool, line, col);
                            }
                        }
                    }
                    MouseEventKind::Up(MouseButton::Right) => {
                        if mouse_event.row == 0 {
                            Self::next_editor(text_editors.len(), current_idx);
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        text_editors[*current_idx].clear_selection();
                        text_editors[*current_idx].move_up();
                    }
                    MouseEventKind::ScrollDown => {
                        text_editors[*current_idx].clear_selection();
                        text_editors[*current_idx].move_down();
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn save(
        text_editors: &mut [&mut dyn TextInteractive],
        file_editors: &mut [&mut dyn FileOperations],
        idx: usize,
        lsp_pool: &[LspClient],
    ) -> io::Result<()> {
        let text: &dyn TextInteractive = &*text_editors[idx];
        file_editors[idx].save_file(text)?;
        file_editors[idx].notify_saved(lsp_pool);
        Ok(())
    }

    fn next_editor(len: usize, current_idx: &mut usize) {
        if len > 0 {
            *current_idx = (*current_idx + 1) % len;
        }
    }

    fn previous_editor(len: usize, current_idx: &mut usize) {
        if len > 0 {
            *current_idx = if *current_idx == 0 {
                len - 1
            } else {
                *current_idx - 1
            };
        }
    }
}

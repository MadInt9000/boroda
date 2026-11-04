use crate::lsp::LspClient;
use crate::traits::{FileOperations, ModalInteractive, TextInteractive};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::io;

pub struct InputDriver;

impl InputDriver {
    /// Обрабатывает ввод только для активного редактора.
    ///
    /// Выбор активного редактора и его переключение — ответственность
    /// `IntermediateState`, поэтому сюда приходят конкретные экземпляры,
    /// а не векторы редакторов.
    pub async fn handle_input(
        text_editor: &mut dyn TextInteractive,
        file_editor: &mut dyn FileOperations,
        lsp_pool: &[LspClient],
        event: Event,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        Self::process_event(text_editor, file_editor, lsp_pool, event).await?;

        // Отображение — тоже часть интерактивного текстового слоя
        text_editor.update_window_size()?;
        text_editor.scroll();
        text_editor.draw(stdout)?;

        Ok(())
    }

    /// Транслирует низкоуровневые клавиши/мышь в смысловые методы
    /// `ModalInteractive` и перерисовывает модалку.
    ///
    /// Проверка результата (`take_result`) и его применение остаются
    /// на стороне `IntermediateState`.
    pub fn handle_modal_input(
        modal: &mut dyn ModalInteractive,
        event: Event,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        match event {
            Event::Key(key) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if let KeyCode::Char(c) = key.code {
                        modal.control_char(c);
                    }
                } else {
                    match key.code {
                        KeyCode::Left => modal.move_left(),
                        KeyCode::Right => modal.move_right(),
                        KeyCode::Up => modal.move_up(),
                        KeyCode::Down => modal.move_down(),
                        KeyCode::PageUp => modal.page_up(),
                        KeyCode::PageDown => modal.page_down(),
                        KeyCode::Home => modal.move_to_start(),
                        KeyCode::End => modal.move_to_end(),
                        KeyCode::Tab => modal.tab(),
                        KeyCode::Char(c) => modal.character_input(c),
                        KeyCode::Backspace => modal.delete_backward(),
                        KeyCode::Delete => modal.delete_forward(),
                        KeyCode::Enter => modal.submit(),
                        KeyCode::Esc => modal.close_modal(),
                        _ => {}
                    }
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    modal.mouse_click(mouse.column, mouse.row)
                }
                MouseEventKind::Down(MouseButton::Right) => modal.close_modal(),
                MouseEventKind::ScrollUp => modal.mouse_scroll(true),
                MouseEventKind::ScrollDown => modal.mouse_scroll(false),
                _ => {}
            },
            _ => {}
        }

        modal.draw(stdout)
    }

    async fn process_event(
        text_editor: &mut dyn TextInteractive,
        file_editor: &mut dyn FileOperations,
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
                        text_editor.begin_selection();
                    } else {
                        text_editor.clear_selection();
                    }
                }

                // CTRL-комбинации
                if has_control {
                    match key.code {
                        KeyCode::Char(c) if c.eq_ignore_ascii_case(&'z') => {
                            if has_shift {
                                text_editor.redo();
                            } else {
                                text_editor.undo();
                            }
                            return Ok(());
                        }
                        KeyCode::Char(c) if c.eq_ignore_ascii_case(&'y') => {
                            text_editor.redo();
                            return Ok(());
                        }
                        KeyCode::Left => {
                            text_editor.move_word_left();
                            return Ok(());
                        }
                        KeyCode::Right => {
                            text_editor.move_word_right();
                            return Ok(());
                        }
                        KeyCode::Insert => {
                            text_editor.paste_from_clipboard();
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // Файловые операции
                match key.code {
                    KeyCode::F(2) => {
                        Self::save(text_editor, file_editor, lsp_pool)?;
                        return Ok(());
                    }
                    KeyCode::F(4) => {
                        let line = text_editor.cursor_y();
                        let col = text_editor.cursor_x();
                        if let Some(lsp) = file_editor.lsp_adapter() {
                            lsp.request_definition(lsp_pool, line, col);
                        }
                        return Ok(());
                    }
                    KeyCode::F(11) => {
                        Self::save(text_editor, file_editor, lsp_pool)?;
                        text_editor.quit();
                        return Ok(());
                    }
                    KeyCode::F(12) => {
                        let lines = file_editor.reload_file()?;
                        text_editor.load_lines(lines);
                        return Ok(());
                    }
                    _ => {}
                }

                // Текстовые операции и навигация
                match key.code {
                    KeyCode::F(1) => text_editor.toggle_indent(),
                    KeyCode::F(5) => text_editor.copy_to_clipboard(),
                    KeyCode::F(6) => text_editor.paste_from_clipboard(),
                    KeyCode::F(7) => text_editor.clear_current_line(),
                    KeyCode::F(8) => text_editor.cut_to_clipboard(),
                    KeyCode::F(9) => text_editor.move_to_line_start(),

                    KeyCode::PageUp => text_editor.page_up(),
                    KeyCode::PageDown => text_editor.page_down(),
                    KeyCode::Home => text_editor.move_to_line_start(),
                    KeyCode::End => text_editor.move_to_line_end(),
                    KeyCode::Up => text_editor.move_up(),
                    KeyCode::Down => text_editor.move_down(),
                    KeyCode::Left => text_editor.move_left(),
                    KeyCode::Right => text_editor.move_right(),

                    KeyCode::Delete => {
                        if text_editor.has_selection() {
                            text_editor.delete_selection();
                        } else {
                            text_editor.delete_char_at_cursor();
                        }
                    }
                    KeyCode::Tab if has_control => {}
                    KeyCode::Tab => {
                        if text_editor.has_selection() {
                            text_editor.delete_selection();
                        }
                        text_editor.insert_str("    ");
                    }
                    KeyCode::Char(c) => {
                        if text_editor.has_selection() {
                            text_editor.delete_selection();
                        }
                        text_editor.insert_char(c);
                    }
                    KeyCode::Backspace => {
                        if text_editor.has_selection() {
                            text_editor.delete_selection();
                        } else {
                            text_editor.delete_char();
                        }
                    }
                    KeyCode::Enter => {
                        if text_editor.has_selection() {
                            text_editor.delete_selection();
                        }
                        text_editor.insert_newline();
                    }
                    _ => {}
                }
            }

            Event::Mouse(mouse_event) => {
                //let has_shift = mouse_event.modifiers.contains(KeyModifiers::SHIFT);
                let has_control = mouse_event.modifiers.contains(KeyModifiers::CONTROL);

                match mouse_event.kind {
                    MouseEventKind::Drag(MouseButton::Left) => {
                        text_editor.click_mouse(mouse_event.column, mouse_event.row);
                        if !text_editor.has_selection() { text_editor.begin_selection(); }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        text_editor.clear_selection();
                        text_editor.click_mouse(mouse_event.column, mouse_event.row);
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if has_control {
                            let line = text_editor.row_offset() + mouse_event.row as usize - 1;
                            let col = text_editor.col_offset() + mouse_event.column as usize;

                            if let Some(lsp) = file_editor.lsp_adapter() {
                                lsp.request_definition(lsp_pool, line, col);
                            }
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        text_editor.clear_selection();
                        text_editor.move_up();
                        text_editor.move_up();
                        text_editor.move_up();
                    }
                    MouseEventKind::ScrollDown => {
                        text_editor.clear_selection();
                        text_editor.move_down();
                        text_editor.move_down();
                        text_editor.move_down();
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn save(
        text_editor: &dyn TextInteractive,
        file_editor: &dyn FileOperations,
        lsp_pool: &[LspClient],
    ) -> io::Result<()> {
        let lines = text_editor.lines();
        file_editor.save_file(lines)?;
        file_editor.notify_saved(lsp_pool);
        Ok(())
    }
}

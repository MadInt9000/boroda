use crate::editor::{Editor, Selection};
use crate::lsp::LspClient;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::io;

pub struct InputDriver;

impl InputDriver {
    pub async fn handle_input(
        editors: &mut Vec<Editor>, 
        current_idx: &mut usize, 
        lsp_pool: &[LspClient], 
        event: Event
    ) -> io::Result<()> {
        let editor = &mut editors[*current_idx];

        match event {
            Event::Key(key) => {
                // 1. Управление выделением при зажатом Shift
                let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);
                let has_control = key.modifiers.contains(KeyModifiers::CONTROL);                 
                // Если зажат Shift и выделение еще не начато — фиксируем стартовую точку
                if has_shift && editor.selection.is_none() {
                    if matches!(key.code, KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) {
                        editor.selection = Some(Selection {
                            start_x: editor.cursor_x,
                            start_y: editor.cursor_y,
                        });
                    }
                }
                // Если нажата навигационная клавиша БЕЗ шифта — сбрасываем выделение
                if !has_shift && matches!(key.code, KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) {
                    editor.selection = None;
                }

                // Обработка CTRL комбинаций
                if has_control {
                    match key.code {
                        KeyCode::PageUp => { InputDriver::previous_editor(editors, current_idx); return Ok(()); }
                        KeyCode::PageDown => { InputDriver::next_editor(editors, current_idx); return Ok(()); }
                        KeyCode::Left => { editor.move_word_left(); return Ok(()); }
                        KeyCode::Right => { editor.move_word_right(); return Ok(()); }
                        KeyCode::Insert => editor.paste_from_clipboard(),
                        _ => {}
                    }
                }

                // Стандартные действия
                match key.code {
                    KeyCode::F(1)  => editor.toggle_indent(),
                    KeyCode::F(2)  => {
                        editor.save_file()?;
                        if let Some(ref lsp) = editor.lsp_adapter { lsp.send_save_notification(lsp_pool); }
                    }
                    KeyCode::F(3)  => editor.search_substring().await,
                    KeyCode::F(4)  => {
                        if let Some(ref lsp) = editor.lsp_adapter {
                            lsp.request_definition(lsp_pool, editor.cursor_y, editor.cursor_x);
                        }
                    }
                    KeyCode::F(7)  => editor.clear_current_line(),
                    KeyCode::F(5)  => editor.copy_to_clipboard(),
                    // Перенаправляем на новые методы умного клипборда
                    KeyCode::F(8)  => editor.cut_to_clipboard(),
                    KeyCode::F(6)  => editor.paste_from_clipboard(),
                    
                    KeyCode::F(9)  => editor.move_to_line_start(),
                    KeyCode::F(10) => editor.quit(),
                    KeyCode::F(11) => {
                        editor.save_file()?;
                        if let Some(ref lsp) = editor.lsp_adapter { lsp.send_save_notification(lsp_pool); }
                        editor.quit();
                    }
                    KeyCode::F(12) => editor.reload_file(),

                    // Навигация (с автоматической поддержкой выделения за счет логики Shift выше)
                    KeyCode::PageUp   => editor.page_up(),
                    KeyCode::PageDown => editor.page_down(),
                    KeyCode::Home     => editor.move_to_line_start(),
                    KeyCode::End      => editor.move_to_line_end(),
                    KeyCode::Up        => editor.move_up(),
                    KeyCode::Down      => editor.move_down(),
                    KeyCode::Left      => editor.move_left(),
                    KeyCode::Right     => editor.move_right(),
                    
                    KeyCode::Delete    => {
                        if editor.selection.is_some() { editor.delete_selection(); } 
                        else { editor.delete_char_at_cursor(); }
                    }                    
                    KeyCode::Tab       => {
                        if editor.selection.is_some() { editor.delete_selection(); }
                        editor.insert_str("    ");
                    }
                    
                    KeyCode::Char(c)   => {
                        if editor.selection.is_some() { editor.delete_selection(); }
                        editor.insert_char(c);
                    }
                    KeyCode::Backspace => {
                        if editor.selection.is_some() { editor.delete_selection(); } 
                        else { editor.delete_char(); }
                    }
                    KeyCode::Enter     => {
                        if editor.selection.is_some() { editor.delete_selection(); }
                        editor.insert_newline();
                    }
                    _ => {}
                }
            }
            
            // 2. Обработка мыши (с поддержкой Shift + клик)
            Event::Mouse(mouse_event) => {
                let has_shift = mouse_event.modifiers.contains(KeyModifiers::SHIFT);
                let has_control = mouse_event.modifiers.contains(KeyModifiers::CONTROL);                 
                
                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if has_shift {
                            // Если зажат Shift и выделение еще не начато, фиксируем старую позицию курсора как старт
                            if editor.selection.is_none() {
                                editor.selection = Some(Selection {
                                    start_x: editor.cursor_x,
                                    start_y: editor.cursor_y,
                                });
                            }
                            // Перемещаем курсор (конец выделения) в точку клика
                            editor.click_mouse(mouse_event.column, mouse_event.row);
                        } else {
                            // Обычный клик сбрасывает выделение и ставит курсор
                            editor.selection = None;
                            editor.click_mouse(mouse_event.column, mouse_event.row);
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if mouse_event.row == 0 { InputDriver::previous_editor(editors, current_idx); }
                        else if has_control {
                            if let Some(ref lsp) = editor.lsp_adapter {
                                
                                lsp.request_definition(
                                    lsp_pool, 
                                    editor.row_offset + mouse_event.row as usize - 1, 
                                    editor.col_offset + mouse_event.column as usize
                                );
                            }
                        }
                    }
                    MouseEventKind::Up(MouseButton::Right) => {
                        if mouse_event.row == 0 { InputDriver::next_editor(editors, current_idx); }
                    }
                    MouseEventKind::ScrollUp => {
                        editor.selection = None;
                        editor.move_up();
                    }
                    MouseEventKind::ScrollDown => {
                        editor.selection = None;
                        editor.move_down();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn next_editor(editors: &mut Vec<Editor>, current_idx: &mut usize) {
        if *current_idx < editors.len() - 1 { *current_idx += 1; } else { *current_idx = 0; }
    }
    
    fn previous_editor(editors: &mut Vec<Editor>, current_idx: &mut usize) {
        if *current_idx > 0 { *current_idx -= 1; } else { *current_idx = editors.len() - 1 }
    }
}


use crate::editor::Editor;
use crate::lsp::LspClient;
use crate::loader::ProjectLoader;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::collections::HashMap;
use std::io;

pub struct InputDriver;

impl InputDriver {
    /// Обрабатывает пользовательский ввод. 
    /// Принимает стек всех редакторов, индекс текущего активного файла и ссылку на пул серверов.
    pub async fn handle_input(
        editors: &mut Vec<Editor>, 
        current_idx: &mut usize, 
        lsp_pool: &HashMap<&'static str, LspClient>,
        event: Event
    ) -> io::Result<()> {
        let editor = &mut editors[*current_idx];

        match event {
            // 1. Обработка клавиатуры
            Event::Key(key) => {
                // Модификатор CTRL: переключение между открытыми файлами (вкладками)
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::PageUp => {
                            if *current_idx > 0 {
                                *current_idx -= 1;
                            } else {
                                *current_idx = editors.len() - 1; // Циклический переход в конец
                            }
                            return Ok(());
                        }
                        KeyCode::PageDown => {
                            if *current_idx < editors.len() - 1 {
                                *current_idx += 1;
                            } else {
                                *current_idx = 0; // Циклический переход в начало
                            }
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // Стандартные горячие клавиши и ввод текста для текущего активного файла
                match key.code {
                    KeyCode::F(1)  => editor.toggle_indent(),
                    
                    // F2: Сохранение файла и отправка didSave уведомления по ссылке из пула
                    KeyCode::F(2)  => {
                        editor.save_file()?;
                        if let (Some(ref lsp), Some(key)) = (&editor.lsp_adapter, ProjectLoader::get_server_key(&editor.filename)) {
                            if let Some(client) = lsp_pool.get(key) {
                                lsp.send_save_notification(client);
                            }
                        }
                    }
                    
                    KeyCode::F(3)  => editor.search_substring().await,
                    KeyCode::F(4)  => { /* Зарезервировано для создания/открытия файла */ }
                    KeyCode::F(5)  => editor.clear_current_line(),
                    KeyCode::F(6)  => { /* Зарезервировано под Info / Go to Definition */ }
                    KeyCode::F(7)  => editor.cut_line(),
                    KeyCode::F(8)  => editor.paste_line(),
                    KeyCode::F(9)  => editor.move_to_line_start(),
                    KeyCode::F(10) => editor.quit(),
                    
                    // F11: Сохранение файла, уведомление LSP и закрытие редактора
                    KeyCode::F(11) => {
                        editor.save_file()?;
                        if let (Some(ref lsp), Some(key)) = (&editor.lsp_adapter, ProjectLoader::get_server_key(&editor.filename)) {
                            if let Some(client) = lsp_pool.get(key) {
                                lsp.send_save_notification(client);
                            }
                        }
                        editor.quit();
                    }
                    
                    KeyCode::F(12) => editor.reload_file(),

                    // Навигация и редактирование
                    KeyCode::PageUp   => editor.page_up(),
                    KeyCode::PageDown => editor.page_down(),
                    KeyCode::Home     => editor.move_to_line_start(),
                    KeyCode::End      => editor.move_to_line_end(),
                    KeyCode::Up        => editor.move_up(),
                    KeyCode::Down      => editor.move_down(),
                    KeyCode::Left      => editor.move_left(),
                    KeyCode::Right     => editor.move_right(),
                    
                    // Табуляция (вставка 4-х пробелов)
                    KeyCode::Tab       => editor.insert_str("    "),
                    
                    // Символьный ввод и удаление
                    KeyCode::Char(c)   => editor.insert_char(c),
                    KeyCode::Backspace => editor.delete_char(),
                    KeyCode::Enter     => editor.insert_newline(),
                    _ => {}
                }
            }
            
            // 2. Обработка мыши
            Event::Mouse(mouse_event) => {
                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        editor.click_mouse(mouse_event.column, mouse_event.row);
                    }
                    MouseEventKind::ScrollUp => {
                        editor.move_up();
                    }
                    MouseEventKind::ScrollDown => {
                        editor.move_down();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

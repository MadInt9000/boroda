use crate::input::InputDriver;
use crate::lsp::LspClient;
use crate::modal::{ModalConfirm, ModalMenu, ModalTextInput};
use crate::tokens::LspDiagnostic;
use crate::traits::{FileOperations, ModalInteractive, ModalResult, TextInteractive};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use std::io;

const SEARCH_MODAL_IDX: usize = 0;
const CONFIRM_EXIT_MODAL_IDX: usize = 1;
const SWITCH_EDITOR_MODAL_IDX: usize = 2;
const DIAGNOSTICS_MODAL_IDX: usize = 3;

/// Действия, которые открывают модальные окна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModalHotkey {
    Search,
    ConfirmExit,
    SwitchEditor,
    Diagnostics,
}

/// Переключение активного редактора без открытия модалки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorSwitch {
    Next,
    Previous,
}

/// Промежуточный слой между обычным вводом и модальными окнами.
///
/// Он сам распознаёт горячие клавиши открытия модалок (F3, F10, Ctrl+Tab),
/// направляет ввод активной модалке и применяет результат её работы
/// к редактору. Главный цикл при этом не знает ни о сигналах, ни о том,
/// какая именно модалка завершилась.
pub struct IntermediateState {
    modals: Vec<Box<dyn ModalInteractive>>,
    active_modal_idx: Option<usize>,
}

impl IntermediateState {
    pub fn new() -> Self {
        Self {
            modals: vec![
                Box::new(ModalTextInput::new("Search")),
                Box::new(ModalConfirm::new("Quit boroda?")),
                Box::new(ModalMenu::new("Opened files")),
                Box::new(ModalMenu::new("Errors & Warnings")),
            ],
            active_modal_idx: None,
        }
    }

    /// Активна ли сейчас хотя бы одна модалка.
    pub fn is_modal_active(&self) -> bool {
        self.active_modal_idx.is_some()
    }

    /// Обрабатывает одно событие ввода.
    ///
    /// Возвращает `true`, если событие было обычным вводом в редактор
    /// и текст мог измениться (нужно перезапустить debounce/LSP).
    pub async fn handle_event(
        &mut self,
        text_editors: &mut Vec<&mut dyn TextInteractive>,
        file_editors: &mut Vec<&mut dyn FileOperations>,
        current_editor_idx: &mut usize,
        lsp_pool: &[LspClient],
        event: Event,
        stdout: &mut io::Stdout,
    ) -> io::Result<bool> {
        if self.is_modal_active() {
            self.handle_modal_event(text_editors, current_editor_idx, event, stdout)?;
            return Ok(false);
        }

        // Передаём текущий редактор напрямую, а не срез `&[&mut dyn ...]`:
        // так helper-методы не индексируют срез трейт-объектов (rust-analyzer
        // ошибочно ругается на такое индексирование).
        let current_editor: &dyn TextInteractive = &*text_editors[*current_editor_idx];
        if let Some(hotkey) = Self::hotkey(&event, current_editor) {
            self.open_modal(
                hotkey,
                current_editor,
                file_editors,
                *current_editor_idx,
                stdout,
            )?;
            return Ok(false);
        }

        // Переключение активного редактора — тоже обязанность этого слоя.
        if let Some(switch) = Self::editor_switch(&event) {
            Self::switch_editor(switch, current_editor_idx, text_editors.len());
            Self::redraw_editor(&mut *text_editors[*current_editor_idx], stdout)?;
            return Ok(false);
        }

        // Драйверу ввода передаются только активные экземпляры.
        let text_editor: &mut dyn TextInteractive = &mut *text_editors[*current_editor_idx];
        let file_editor: &mut dyn FileOperations = &mut *file_editors[*current_editor_idx];

        InputDriver::handle_input(text_editor, file_editor, lsp_pool, event, stdout).await?;

        Ok(true)
    }

    fn hotkey(event: &Event, current_editor: &dyn TextInteractive) -> Option<ModalHotkey> {
        match event {
            Event::Key(key) => match key.code {
                KeyCode::F(3) => Some(ModalHotkey::Search),
                KeyCode::F(10) => Some(ModalHotkey::ConfirmExit),
                KeyCode::F(9) => Some(ModalHotkey::SwitchEditor),
                _ => None,
            },
            // Клик по верхней строке открывает меню файлов, а клик по значкам
            // ошибок/предупреждений — меню диагностик.
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Up(MouseButton::Left) if mouse.row == 0 => {
                    if current_editor.diagnostics_icon_at(mouse.column, mouse.row) {
                        Some(ModalHotkey::Diagnostics)
                    } else {
                        Some(ModalHotkey::SwitchEditor)
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Распознаёт события, которые переключают активный редактор.
    ///
    /// Это Ctrl+PageUp/PageDown и правый клик по строке заголовка.
    fn editor_switch(event: &Event) -> Option<EditorSwitch> {
        match event {
            Event::Key(key) if key.modifiers.contains(KeyModifiers::CONTROL) => match key.code {
                KeyCode::PageUp => Some(EditorSwitch::Previous),
                KeyCode::PageDown => Some(EditorSwitch::Next),
                _ => None,
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Up(MouseButton::Right) if mouse.row == 0 => {
                    Some(EditorSwitch::Next)
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn switch_editor(switch: EditorSwitch, current_editor_idx: &mut usize, count: usize) {
        if count == 0 {
            return;
        }

        *current_editor_idx = match switch {
            EditorSwitch::Next => (*current_editor_idx + 1) % count,
            EditorSwitch::Previous => {
                if *current_editor_idx == 0 {
                    count - 1
                } else {
                    *current_editor_idx - 1
                }
            }
        };
    }

    fn open_modal(
        &mut self,
        hotkey: ModalHotkey,
        current_editor: &dyn TextInteractive,
        file_editors: &[&mut dyn FileOperations],
        current_editor_idx: usize,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        let index = match hotkey {
            ModalHotkey::Search => SEARCH_MODAL_IDX,
            ModalHotkey::ConfirmExit => CONFIRM_EXIT_MODAL_IDX,
            ModalHotkey::SwitchEditor => SWITCH_EDITOR_MODAL_IDX,
            ModalHotkey::Diagnostics => DIAGNOSTICS_MODAL_IDX,
        };

        self.modals[index].open();

        match hotkey {
            ModalHotkey::SwitchEditor => {
                let items = file_editors
                    .iter()
                    .map(|file| file.filename().to_string())
                    .collect();
                self.modals[index].set_items(items);
                self.modals[index].set_selected(current_editor_idx);
            }
            ModalHotkey::Diagnostics => {
                let items = current_editor
                    .diagnostics()
                    .iter()
                    .map(Self::diagnostic_label)
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    return Ok(());
                }
                self.modals[index].set_items(items);
            }
            _ => {}
        }

        self.active_modal_idx = Some(index);
        self.modals[index].draw(stdout)
    }

    fn diagnostic_label(diagnostic: &LspDiagnostic) -> String {
        let marker = match diagnostic.severity {
            1 => "🛑",
            2 => "⚠️",
            _ => "📝",
        };
        format!(
            "{} L{}:{} {}",
            marker,
            diagnostic.start_line + 1,
            diagnostic.start_col + 1,
            diagnostic.message.replace('\n', " ")
        )
    }

    fn handle_modal_event(
        &mut self,
        text_editors: &mut Vec<&mut dyn TextInteractive>,
        current_editor_idx: &mut usize,
        event: Event,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        let Some(index) = self.active_modal_idx else {
            return Ok(());
        };

        let result = {
            let modal = &mut *self.modals[index];
            InputDriver::handle_modal_input(modal, event, stdout)?;
            modal.take_result()
        };

        let Some(result) = result else {
            return Ok(());
        };

        self.active_modal_idx = None;

        if result == ModalResult::Submit {
            self.apply_modal_result(index, text_editors, current_editor_idx);
        }

        Self::redraw_editor(&mut *text_editors[*current_editor_idx], stdout)
    }

    fn apply_modal_result(
        &mut self,
        index: usize,
        text_editors: &mut Vec<&mut dyn TextInteractive>,
        current_editor_idx: &mut usize,
    ) {
        match index {
            SEARCH_MODAL_IDX => {
                let query = self.modals[index].text().unwrap_or_default();
                text_editors[*current_editor_idx].find_in_editor(&query);
            }
            CONFIRM_EXIT_MODAL_IDX => {
                if self.modals[index].confirmed() {
                    text_editors[*current_editor_idx].quit();
                }
            }
            SWITCH_EDITOR_MODAL_IDX => {
                if let Some(selected) = self.modals[index].selected() {
                    if selected < text_editors.len() {
                        *current_editor_idx = selected;
                    }
                }
            }
            DIAGNOSTICS_MODAL_IDX => {
                if let Some(selected) = self.modals[index].selected() {
                    text_editors[*current_editor_idx].jump_to_diagnostic(selected);
                }
            }
            _ => {}
        }
    }

    fn redraw_editor(editor: &mut dyn TextInteractive, stdout: &mut io::Stdout) -> io::Result<()> {
        editor.update_window_size()?;
        editor.scroll();
        editor.draw(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_label_contains_marker_position_and_message() {
        let diagnostic = LspDiagnostic {
            start_line: 4,
            start_col: 2,
            end_line: 4,
            end_col: 5,
            severity: 2,
            message: "first\nsecond".to_string(),
        };

        let label = IntermediateState::diagnostic_label(&diagnostic);

        assert!(label.contains('⚠'));
        assert!(label.contains("L5:3"));
        assert!(label.contains("first second"));
    }
}

use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;
use crate::tokens::{LspDiagnostic, RenderToken};
use std::io;

/// Всё, что относится к интерактивному тексту:
/// курсор, ввод, выделение, блоки, поиск и отрисовка.
pub trait TextInteractive {
    fn title(&self) -> &str;
    fn lines(&self) -> &Vec<Vec<char>>;
    fn get_all_text(&self) -> String;
    fn load_lines(&mut self, lines: Vec<Vec<char>>);

    fn update_window_size(&mut self) -> io::Result<()>;
    fn scroll(&mut self);
    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()>;
    fn update_semantic_tokens(&mut self, tokens: Vec<Vec<RenderToken>>);
    fn set_diagnostics(&mut self, errors: usize, warnings: usize, diagnostics: Vec<LspDiagnostic>);
    fn diagnostics(&self) -> &[LspDiagnostic];

    /// Попал ли клик в значки ошибок/предупреждений в строке заголовка.
    fn diagnostics_icon_at(&self, x: u16, y: u16) -> bool;
    /// Перемещает курсор на диагностику с указанным индексом.
    fn jump_to_diagnostic(&mut self, index: usize);

    fn cursor_x(&self) -> usize;
    fn cursor_y(&self) -> usize;
    fn row_offset(&self) -> usize;
    fn col_offset(&self) -> usize;
    fn set_cursor(&mut self, y: usize, x: usize);

    /// Ищет `query` начиная с текущей строки с переносом в начало
    /// и перемещает курсор на найденный фрагмент.
    fn find_in_editor(&mut self, query: &str);

    fn has_selection(&self) -> bool;
    fn begin_selection(&mut self);
    fn clear_selection(&mut self);
    fn click_mouse(&mut self, x: u16, y: u16);

    fn move_up(&mut self);
    fn move_down(&mut self);
    fn move_left(&mut self);
    fn move_right(&mut self);
    fn page_up(&mut self);
    fn page_down(&mut self);
    fn move_to_line_start(&mut self);
    fn move_to_line_end(&mut self);
    fn move_word_left(&mut self);
    fn move_word_right(&mut self);

    fn insert_char(&mut self, c: char);
    fn insert_str(&mut self, s: &str);
    fn insert_newline(&mut self);
    fn delete_char(&mut self);
    fn delete_char_at_cursor(&mut self);
    fn delete_selection(&mut self);
    fn clear_current_line(&mut self);
    fn toggle_indent(&mut self);

    fn copy_to_clipboard(&self);
    fn cut_to_clipboard(&mut self);
    fn paste_from_clipboard(&mut self);

    fn undo(&mut self);
    fn redo(&mut self);

    fn should_quit(&self) -> bool;
    fn quit(&mut self);
}

/// Результат закрытия модального окна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalResult {
    /// Пользователь подтвердил действие (Enter, клик по кнопке).
    Submit,
    /// Пользователь отменил действие (Esc).
    Cancel,
}

/// Общий интерфейс модальных окон.
///
/// `InputDriver` транслирует клавиши и мышь в эти смысловые действия,
/// а конкретные модалки сами решают, что с ними делать.
pub trait ModalInteractive {
    // Навигация
    fn move_left(&mut self) {}
    fn move_right(&mut self) {}
    fn move_up(&mut self) {}
    fn move_down(&mut self) {}
    fn page_up(&mut self) {}
    fn page_down(&mut self) {}
    fn move_to_start(&mut self) {}
    fn move_to_end(&mut self) {}
    fn tab(&mut self) {}

    // Ввод
    fn character_input(&mut self, _c: char) {}
    fn control_char(&mut self, _c: char) {}
    fn delete_backward(&mut self) {}
    fn delete_forward(&mut self) {}

    // Завершение
    fn submit(&mut self);
    fn close_modal(&mut self);

    // Мышь
    fn mouse_click(&mut self, _x: u16, _y: u16) {}
    fn mouse_scroll(&mut self, _up: bool) {}

    // Жизненный цикл и данные результата
    fn open(&mut self);
    fn set_items(&mut self, _items: Vec<String>) {}
    fn set_selected(&mut self, _index: usize) {}

    fn text(&self) -> Option<String> {
        None
    }
    fn confirmed(&self) -> bool {
        false
    }
    fn selected(&self) -> Option<usize> {
        None
    }

    /// Возвращает результат, если модалка закрылась через submit/close_modal.
    fn take_result(&mut self) -> Option<ModalResult>;

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()>;
}

/// Всё, что относится к файлу на диске и его LSP-контексту.
pub trait FileOperations {
    fn filename(&self) -> &str;
    fn save_file(&self, lines: &[Vec<char>]) -> io::Result<()>;
    fn reload_file(&self) -> io::Result<Vec<Vec<char>>>;
    fn notify_saved(&self, lsp_pool: &[LspClient]);
    fn lsp_adapter(&self) -> Option<&LspAdapter>;
}

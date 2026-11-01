use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;
use crate::tokens::{LspDiagnostic, RenderToken};
use std::future::Future;
use std::io;
use std::pin::Pin;

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
    fn set_diagnostics(
        &mut self,
        errors: usize,
        warnings: usize,
        diagnostics: Vec<LspDiagnostic>,
    );

    fn cursor_x(&self) -> usize;
    fn cursor_y(&self) -> usize;
    fn row_offset(&self) -> usize;
    fn col_offset(&self) -> usize;
    fn set_cursor(&mut self, y: usize, x: usize);

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

    fn search_substring<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = ()> + 'a>>;

    fn should_quit(&self) -> bool;
    fn quit(&mut self);
}

/// Всё, что относится к файлу на диске и его LSP-контексту.
pub trait FileOperations {
    fn filename(&self) -> &str;
    fn save_file(&self, text: &dyn TextInteractive) -> io::Result<()>;
    fn reload_file(&mut self, text: &mut dyn TextInteractive);
    fn notify_saved(&self, lsp_pool: &[LspClient]);
    fn lsp_adapter(&self) -> Option<&LspAdapter>;
}

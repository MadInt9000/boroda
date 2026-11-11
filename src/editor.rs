use crate::base_editor::BaseEditor;
use crate::config::Configuration;
use crate::lsp::LspClient;
use crate::lsp_adapter::LspAdapter;
use crate::tokens::{LspDiagnostic, RenderToken};
use crate::traits::{FileOperations, TextInteractive};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

/// Файловая половина редактора: путь и LSP-адаптер.
pub struct FileContext {
    pub filename: String,
    pub lsp_adapter: Option<LspAdapter>,
}

impl FileContext {
    pub fn new(filename: String) -> Self {
        Self {
            filename,
            lsp_adapter: None,
        }
    }
}

/// Editor — это композиция интерактивного текста (BaseEditor)
/// и файловых операций (FileContext).
pub struct Editor {
    pub base: BaseEditor,
    pub file: FileContext,
}

impl Editor {
    pub fn new(filename: String) -> Self {
        let ext = Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let color_palette = Configuration::global().compile_palette_to_vector(ext);

        let mut editor = Self {
            base: BaseEditor::new(filename.clone(), color_palette),
            file: FileContext::new(filename),
        };
        if let Ok(lines) = editor.file.reload_file() {
            editor.base.load_lines(lines);
        }
        editor
    }

    /// Собирает два вектора трейт-объектов из одного Vec<Editor>.
    /// Оба вектора индексируются одинаково через active_editor.
    pub fn split_views<'a>(
        editors: &'a mut [Editor],
    ) -> (
        Vec<&'a mut dyn TextInteractive>,
        Vec<&'a mut dyn FileOperations>,
    ) {
        let mut text_editors = Vec::with_capacity(editors.len());
        let mut file_editors = Vec::with_capacity(editors.len());

        for editor in editors.iter_mut() {
            let text: &mut dyn TextInteractive = &mut editor.base;
            let file: &mut dyn FileOperations = &mut editor.file;
            text_editors.push(text);
            file_editors.push(file);
        }

        (text_editors, file_editors)
    }
}

impl TextInteractive for Editor {
    fn title(&self) -> String {
        self.base.title()
    }

    fn lines(&self) -> &Vec<Vec<char>> {
        self.base.lines()
    }

    fn get_all_text(&self) -> String {
        self.base.get_all_text()
    }

    fn load_lines(&mut self, lines: Vec<Vec<char>>) {
        self.base.load_lines(lines)
    }

    fn mark_saved(&mut self) {
        self.base.mark_saved()
    }

    fn update_window_size(&mut self) -> io::Result<()> {
        self.base.update_window_size()
    }

    fn scroll(&mut self) {
        self.base.scroll()
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        self.base.draw(stdout)
    }

    fn update_semantic_tokens(&mut self, tokens: Vec<Vec<RenderToken>>) {
        self.base.update_semantic_tokens(tokens)
    }

    fn set_diagnostics(&mut self, errors: usize, warnings: usize, diagnostics: Vec<LspDiagnostic>) {
        self.base.set_diagnostics(errors, warnings, diagnostics)
    }

    fn diagnostics(&self) -> &[LspDiagnostic] {
        self.base.diagnostics()
    }

    fn diagnostics_icon_at(&self, x: u16, y: u16) -> bool {
        self.base.diagnostics_icon_at(x, y)
    }

    fn jump_to_diagnostic(&mut self, index: usize) {
        self.base.jump_to_diagnostic(index)
    }

    fn cursor_x(&self) -> usize {
        self.base.cursor_x()
    }

    fn cursor_y(&self) -> usize {
        self.base.cursor_y()
    }

    fn row_offset(&self) -> usize {
        self.base.row_offset()
    }

    fn col_offset(&self) -> usize {
        self.base.col_offset()
    }

    fn set_cursor(&mut self, y: usize, x: usize) {
        self.base.set_cursor(y, x)
    }

    fn find_in_editor(&mut self, query: &str) {
        self.base.find_in_editor(query)
    }

    fn has_selection(&self) -> bool {
        self.base.has_selection()
    }

    fn begin_selection(&mut self) {
        self.base.begin_selection()
    }

    fn clear_selection(&mut self) {
        self.base.clear_selection()
    }

    fn click_mouse(&mut self, x: u16, y: u16) {
        self.base.click_mouse(x, y)
    }

    fn move_up(&mut self) {
        self.base.move_up()
    }

    fn move_down(&mut self) {
        self.base.move_down()
    }

    fn move_left(&mut self) {
        self.base.move_left()
    }

    fn move_right(&mut self) {
        self.base.move_right()
    }

    fn page_up(&mut self) {
        self.base.page_up()
    }

    fn page_down(&mut self) {
        self.base.page_down()
    }

    fn move_to_line_start(&mut self) {
        self.base.move_to_line_start()
    }

    fn move_to_line_end(&mut self) {
        self.base.move_to_line_end()
    }

    fn move_word_left(&mut self) {
        self.base.move_word_left()
    }

    fn move_word_right(&mut self) {
        self.base.move_word_right()
    }

    fn insert_char(&mut self, c: char) {
        self.base.insert_char(c)
    }

    fn insert_str(&mut self, s: &str) {
        self.base.insert_str(s)
    }

    fn insert_newline(&mut self) {
        self.base.insert_newline()
    }

    fn delete_char(&mut self) {
        self.base.delete_char()
    }

    fn delete_char_at_cursor(&mut self) {
        self.base.delete_char_at_cursor()
    }

    fn delete_selection(&mut self) {
        self.base.delete_selection()
    }

    fn clear_current_line(&mut self) {
        self.base.clear_current_line()
    }

    fn toggle_indent(&mut self) {
        self.base.toggle_indent()
    }

    fn copy_to_clipboard(&self) {
        self.base.copy_to_clipboard()
    }

    fn cut_to_clipboard(&mut self) {
        self.base.cut_to_clipboard()
    }

    fn paste_from_clipboard(&mut self) {
        self.base.paste_from_clipboard()
    }

    fn undo(&mut self) {
        self.base.undo()
    }

    fn redo(&mut self) {
        self.base.redo()
    }

    fn should_quit(&self) -> bool {
        self.base.should_quit()
    }

    fn quit(&mut self) {
        self.base.quit()
    }
}

impl FileOperations for FileContext {
    fn filename(&self) -> &str {
        &self.filename
    }

    fn save_file(&self, lines: &[Vec<char>]) -> io::Result<()> {
        let mut file = File::create(&self.filename)?;
        for line in lines {
            let s: String = line.iter().collect();
            writeln!(file, "{}", s)?;
        }
        Ok(())
    }

    fn reload_file(&self) -> io::Result<Vec<Vec<char>>> {
        let mut new_lines = Vec::new();
        let path = Path::new(&self.filename);

        if path.exists() {
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                new_lines.push(line?.chars().collect());
            }
        }

        Ok(new_lines)
    }

    fn notify_saved(&self, lsp_pool: &[LspClient]) {
        if let Some(ref lsp) = self.lsp_adapter {
            lsp.send_save_notification(lsp_pool);
        }
    }

    fn lsp_adapter(&self) -> Option<&LspAdapter> {
        self.lsp_adapter.as_ref()
    }
}

impl FileOperations for Editor {
    fn filename(&self) -> &str {
        self.file.filename()
    }

    fn save_file(&self, lines: &[Vec<char>]) -> io::Result<()> {
        self.file.save_file(lines)
    }

    fn reload_file(&self) -> io::Result<Vec<Vec<char>>> {
        self.file.reload_file()
    }

    fn notify_saved(&self, lsp_pool: &[LspClient]) {
        self.file.notify_saved(lsp_pool)
    }

    fn lsp_adapter(&self) -> Option<&LspAdapter> {
        self.file.lsp_adapter()
    }
}

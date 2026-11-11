use crate::clipboard::ClipboardManager;
use crate::history::{History, Operation};
use crate::tokens::{LspDiagnostic, RenderToken, Selection};
use crate::traits::TextInteractive;
use crossterm::{cursor, style::Print, terminal, QueueableCommand};
use std::cmp;
use std::io::{self, Write};

pub struct BaseEditor {
    pub filename: String,
    pub lines: Vec<Vec<char>>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub row_offset: usize,
    pub col_offset: usize,
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub has_indent: bool,
    pub should_quit: bool,
    /// Были ли изменения с момента последней загрузки/сохранения.
    pub modified: bool,
    pub selection: Option<Selection>,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<LspDiagnostic>,
    // Каждому элементу lines соответствует свой вектор токенов
    pub semantic_tokens: Vec<Vec<RenderToken>>,
    pub color_palette: Vec<String>,
    pub history: History,
}

impl BaseEditor {
    pub fn new(filename: String, color_palette: Vec<String>) -> Self {
        Self {
            filename,
            lines: vec![Vec::new()],
            cursor_x: 0,
            cursor_y: 0,
            row_offset: 0,
            col_offset: 0,
            screen_cols: 0,
            screen_rows: 0,
            has_indent: true,
            should_quit: false,
            modified: false,
            selection: None,
            error_count: 0,
            warning_count: 0,
            diagnostics: Vec::new(),
            semantic_tokens: vec![Vec::new()],
            color_palette,
            history: History::new(),
        }
    }

    fn line_string(&self, row: usize) -> String {
        self.lines
            .get(row)
            .map(|line| line.iter().collect())
            .unwrap_or_default()
    }

    /// Синхронизирует служебное состояние после undo/redo.
    fn sync_after_history(&mut self) {
        self.semantic_tokens = vec![Vec::new(); self.lines.len()];
        self.selection = None;
        self.cursor_y = self.cursor_y.min(self.lines.len().saturating_sub(1));
        self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.lines, &mut self.cursor_y) {
            self.sync_after_history();
            self.modified = true;
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.lines, &mut self.cursor_y) {
            self.sync_after_history();
            self.modified = true;
        }
    }

    pub fn get_all_text(&self) -> String {
        let mut full_text = String::new();
        for line in &self.lines {
            let s: String = line.iter().collect();
            full_text.push_str(&s);
            full_text.push('\n');
        }
        full_text
    }

    pub fn load_lines(&mut self, new_lines: Vec<Vec<char>>) {
        self.history.clear();
        self.lines = if new_lines.is_empty() {
            vec![Vec::new()]
        } else {
            new_lines
        };

        // Синхронизируем размер индекса токенов с количеством строк
        self.semantic_tokens = vec![Vec::new(); self.lines.len()];
        self.selection = None;
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_offset = 0;
        self.col_offset = 0;
        self.diagnostics.clear();
        self.modified = false;
    }

    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    pub fn update_window_size(&mut self) -> io::Result<()> {
        let (cols, rows) = terminal::size()?;
        self.screen_cols = cols;
        self.screen_rows = rows;
        Ok(())
    }

    pub fn update_semantic_tokens(&mut self, tokens: Vec<Vec<RenderToken>>) {
        self.semantic_tokens = tokens;
    }

    pub fn set_diagnostics(
        &mut self,
        errors: usize,
        warnings: usize,
        diagnostics: Vec<LspDiagnostic>,
    ) {
        self.error_count = errors;
        self.warning_count = warnings;
        self.diagnostics = diagnostics;
    }

    fn header_left(&self) -> String {
        format!(
            " {} | Lines: {} | Pos: {},{} | ",
            self.title(),
            self.lines.len(),
            self.cursor_y + 1,
            self.cursor_x + 1
        )
    }

    fn header_icons(&self) -> String {
        let mut icons = String::new();
        for _ in 0..self.error_count {
            icons.push_str("🛑");
        }
        for _ in 0..self.warning_count {
            icons.push_str("⚠️ ");
        }
        icons
    }

    fn diagnostics_icon_width(&self) -> usize {
        if self.error_count == 0 && self.warning_count == 0 {
            0
        } else {
            self.error_count * 2 + self.warning_count * 2 + 1
        }
    }

    /// Попал ли клик в значки ошибок/предупреждений в строке заголовка.
    pub fn diagnostics_icon_at(&self, x: u16, y: u16) -> bool {
        if y != 0 {
            return false;
        }

        let width = self.diagnostics_icon_width();
        if width == 0 {
            return false;
        }

        let start = self.header_left().chars().count() as u16;
        let end = start.saturating_add(width as u16);
        x >= start && x < end
    }

    /// Перемещает курсор на диагностику с указанным индексом.
    pub fn jump_to_diagnostic(&mut self, index: usize) {
        if let Some(diagnostic) = self.diagnostics.get(index) {
            let (line, col) = (diagnostic.start_line, diagnostic.start_col);
            self.set_cursor(line, col);
        }
    }

    pub fn scroll(&mut self) {
        let text_rows = self.screen_rows.saturating_sub(2) as usize;

        if self.cursor_y < self.row_offset {
            self.row_offset = self.cursor_y;
        }
        if self.cursor_y >= self.row_offset + text_rows {
            self.row_offset = self.cursor_y - text_rows + 1;
        }

        if self.cursor_x < self.col_offset {
            self.col_offset = self.cursor_x;
        }
        if self.cursor_x >= self.col_offset + self.screen_cols as usize {
            self.col_offset = self.cursor_x - self.screen_cols as usize + 1;
        }
    }

    fn is_error_char(&self, row: usize, col: usize) -> bool {
        for diag in &self.diagnostics {
            if row > diag.start_line && row < diag.end_line {
                return true;
            }
            if row == diag.start_line && row < diag.end_line && col >= diag.start_col {
                return true;
            }
            if row == diag.end_line && row > diag.start_line && col < diag.end_col {
                return true;
            }
            if row == diag.start_line && row == diag.end_line {
                if col >= diag.start_col && col < diag.end_col {
                    return true;
                }
                if let Some(line) = self.lines.get(row) {
                    if diag.start_col >= line.len() && col == line.len().saturating_sub(1) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn is_inside_selection(&self, row: usize, col: usize) -> bool {
        if let Some(sel) = self.selection {
            // Нормализуем start и end, так как выделять можно в любом направлении
            let (p1_y, p1_x) = (sel.start_y, sel.start_x);
            let (p2_y, p2_x) = (self.cursor_y, self.cursor_x);

            let (start_y, start_x, end_y, end_x) = if p1_y < p2_y || (p1_y == p2_y && p1_x <= p2_x)
            {
                (p1_y, p1_x, p2_y, p2_x)
            } else {
                (p2_y, p2_x, p1_y, p1_x)
            };

            if row < start_y || row > end_y {
                return false;
            }
            if row == start_y && row == end_y {
                return col >= start_x && col < end_x;
            }
            if row == start_y {
                return col >= start_x;
            }
            if row == end_y {
                return col < end_x;
            }
            return true;
        }
        false
    }

    fn get_token_color(&self, token_type: u32) -> &str {
        let idx = token_type as usize;

        if idx < self.color_palette.len() {
            &self.color_palette[idx]
        } else {
            "\x1b[0m"
        }
    }

    /// Если курсор стоит на скобке, возвращает её позицию и позицию парной скобки.
    fn matching_brackets(&self) -> Option<((usize, usize), (usize, usize))> {
        let line = self.lines.get(self.cursor_y)?;
        let bracket = *line.get(self.cursor_x)?;

        let (open, close) = match bracket {
            '(' => ('(', ')'),
            ')' => ('(', ')'),
            '[' => ('[', ']'),
            ']' => ('[', ']'),
            '{' => ('{', '}'),
            '}' => ('{', '}'),
            _ => return None,
        };

        let matched = if bracket == open {
            self.find_bracket_forward(self.cursor_y, self.cursor_x, open, close)?
        } else {
            self.find_bracket_backward(self.cursor_y, self.cursor_x, open, close)?
        };

        Some(((self.cursor_y, self.cursor_x), matched))
    }

    fn find_bracket_forward(
        &self,
        start_y: usize,
        start_x: usize,
        open: char,
        close: char,
    ) -> Option<(usize, usize)> {
        let mut depth = 0usize;

        for y in start_y..self.lines.len() {
            let line = &self.lines[y];
            let start = if y == start_y { start_x } else { 0 };

            for (x, &c) in line.iter().enumerate().skip(start) {
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((y, x));
                    }
                }
            }
        }

        None
    }

    fn find_bracket_backward(
        &self,
        start_y: usize,
        start_x: usize,
        open: char,
        close: char,
    ) -> Option<(usize, usize)> {
        let mut depth = 0usize;
        let mut y = start_y;
        let mut x = start_x + 1;

        loop {
            while x > 0 {
                x -= 1;
                let c = self.lines[y][x];

                if c == close {
                    depth += 1;
                } else if c == open {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((y, x));
                    }
                }
            }

            if y == 0 {
                return None;
            }
            y -= 1;
            x = self.lines[y].len();
        }
    }

    fn draw_line_with_lsp_syntax(
        &self,
        file_row: usize,
        render_len: usize,
        matching_brackets: Option<((usize, usize), (usize, usize))>,
        stdout: &mut io::Stdout,
    ) -> io::Result<()> {
        let line = &self.lines[file_row];
        let row_tokens = self.semantic_tokens.get(file_row);
        let mut token_idx = 0;
        let mut output = String::new();

        for col_idx in 0..render_len {
            let file_col = col_idx + self.col_offset;
            if file_col >= line.len() {
                break;
            }
            let c = line[file_col];

            let is_err = self.is_error_char(file_row, file_col);
            let is_sel = self.is_inside_selection(file_row, file_col);
            let is_bracket = matches!(
                matching_brackets,
                Some((first, second))
                    if first == (file_row, file_col) || second == (file_row, file_col)
            );

            if is_sel {
                output.push_str("\x1b[7m");
                output.push(c);
                output.push_str("\x1b[m");
                continue;
            }

            if is_bracket {
                output.push_str("\x1b[1;30;46m");
                output.push(c);
                output.push_str("\x1b[0m");
            } else if is_err {
                output.push_str("\x1b[41m");
                output.push(c);
                output.push_str("\x1b[0m");
            } else {
                let mut color = "\x1b[0m";
                if let Some(tokens) = row_tokens {
                    while token_idx < tokens.len()
                        && file_col >= tokens[token_idx].start_col + tokens[token_idx].len
                    {
                        token_idx += 1;
                    }
                    if token_idx < tokens.len() {
                        let token = &tokens[token_idx];
                        if file_col >= token.start_col && file_col < token.start_col + token.len {
                            color = self.get_token_color(token.token_type);
                        }
                    }
                }

                output.push_str(color);
                output.push(c);
                output.push_str("\x1b[0m");
            }
        }

        stdout.queue(Print(output))?;
        Ok(())
    }

    pub fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        let _ = stdout.queue(cursor::Hide);

        // 1. Статус-бар
        stdout.queue(cursor::MoveTo(0, 0))?;

        let status_left = self.header_left();
        let status_right = self.header_icons();
        let right_visual_len = self.diagnostics_icon_width();
        let total_used = status_left.len() + right_visual_len;

        let fill_len = (self.screen_cols as usize).saturating_sub(total_used);
        let fill = " ".repeat(fill_len);

        let full_status = format!("{}{}{}", status_left, status_right, fill);

        let render_status = if full_status.chars().count() > self.screen_cols as usize {
            full_status
                .chars()
                .take(self.screen_cols as usize)
                .collect::<String>()
        } else {
            full_status
        };
        stdout.queue(Print(format!("\x1b[7m{}\x1b[m", render_status)))?;

        // 2. Отображение текста с подсветкой синтаксиса
        let matching_brackets = self.matching_brackets();
        let text_rows = self.screen_rows.saturating_sub(2) as usize;
        for i in 0..text_rows {
            let file_row = i + self.row_offset;
            stdout.queue(cursor::MoveTo(0, i as u16 + 1))?;

            if file_row < self.lines.len() {
                let line = &self.lines[file_row];
                if line.len() > self.col_offset {
                    let render_len = (line.len() - self.col_offset).min(self.screen_cols as usize);
                    self.draw_line_with_lsp_syntax(
                        file_row,
                        render_len,
                        matching_brackets,
                        stdout,
                    )?;
                }
            }

            stdout.queue(Print("\x1b[K"))?;
        }

        // 3. Подсказки
        stdout.queue(cursor::MoveTo(0, self.screen_rows - 1))?;
        let mut active_error_msg = None;
        for diag in &self.diagnostics {
            if self.cursor_y == diag.start_line {
                active_error_msg = Some(format!("⚠️ {}", diag.message.replace('\n', " ")));
                break;
            }
        }

        let footer_text = active_error_msg.unwrap_or_else(|| {
            // 12 хинтов растягиваем на всю ширину терминала:
            // каждому хинту достается screen_cols / 12 знакомест,
            // недостающие пробелы добавляем справа.
            let hints = [
                "F1 TogIdt",
                "F2 Save",
                "F3 Search",
                "F4 GoDef",
                "F5 Copy",
                "F6 Paste",
                "F7 Clear",
                "F8 Cut",
                "F9 OpnFls",
                "F10 Quit",
                "F11 ExSave",
                "F12 Reload",
            ];
            let screen_cols = self.screen_cols as usize;
            let slot_width = screen_cols / hints.len();
            let mut bar = String::with_capacity(screen_cols);

            for hint in hints {
                let used = hint.chars().count();
                bar.push_str(hint);
                if used < slot_width {
                    bar.push_str(&" ".repeat(slot_width - used));
                }
            }

            // Остаток от деления ширины на 12 добавляем в конец
            let bar_len = bar.chars().count();
            if bar_len < screen_cols {
                bar.push_str(&" ".repeat(screen_cols - bar_len));
            }

            bar
        });

        let screen_cols = self.screen_cols as usize;
        let render_hints: String = if footer_text.chars().count() > screen_cols {
            footer_text.chars().take(screen_cols).collect()
        } else {
            footer_text
        };
        let fill_hint = " ".repeat(screen_cols.saturating_sub(render_hints.chars().count()));
        stdout.queue(Print(format!("\x1b[7m{}{}\x1b[m", render_hints, fill_hint)))?;

        let render_x = self.cursor_x.saturating_sub(self.col_offset) as u16;
        let render_y = (self.cursor_y.saturating_sub(self.row_offset) + 1) as u16;
        stdout.queue(cursor::MoveTo(render_x, render_y))?;
        stdout.queue(cursor::Show)?;
        stdout.flush()?;
        Ok(())
    }

    pub fn click_mouse(&mut self, x: u16, y: u16) {
        let text_rows = self.screen_rows.saturating_sub(2);
        if y >= 1 && y <= text_rows {
            let target_file_row = (y - 1) as usize + self.row_offset;
            if target_file_row < self.lines.len() {
                self.cursor_y = target_file_row;
                let target_file_col = x as usize + self.col_offset;
                self.cursor_x = target_file_col.min(self.lines[self.cursor_y].len());
            }
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn clear_current_line(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        if self.lines[self.cursor_y].is_empty() {
            return;
        }

        let old = self.line_string(self.cursor_y);
        self.history.record(Operation::Change, self.cursor_y, old);
        self.lines[self.cursor_y].clear();
        self.semantic_tokens[self.cursor_y].clear();
        self.cursor_x = 0;
        self.modified = true;
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor_x = 0;
    }

    pub fn move_to_line_end(&mut self) {
        self.cursor_x = self.lines[self.cursor_y].len();
    }

    pub fn page_up(&mut self) {
        let steps = self.screen_rows.saturating_sub(2) as usize;
        self.cursor_y = self.cursor_y.saturating_sub(steps);
        self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
    }

    pub fn page_down(&mut self) {
        let steps = self.screen_rows.saturating_sub(2) as usize;
        self.cursor_y = (self.cursor_y + steps).min(self.lines.len() - 1);
        self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
    }

    pub fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_x < self.lines[self.cursor_y].len() {
            self.cursor_x += 1;
        } else if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let old = self.line_string(self.cursor_y);
        self.history.record(Operation::Change, self.cursor_y, old);
        self.lines[self.cursor_y].insert(self.cursor_x, c);
        self.cursor_x += 1;
        self.modified = true;
    }

    pub fn delete_char_at_cursor(&mut self) {
        if self.cursor_x < self.lines[self.cursor_y].len() {
            let old = self.line_string(self.cursor_y);
            self.history.record(Operation::Change, self.cursor_y, old);
            self.lines[self.cursor_y].remove(self.cursor_x);
            self.modified = true;
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor_x > 0 {
            let old = self.line_string(self.cursor_y);
            self.history.record(Operation::Change, self.cursor_y, old);
            self.lines[self.cursor_y].remove(self.cursor_x - 1);
            self.cursor_x -= 1;
            self.modified = true;
        } else if self.cursor_y > 0 {
            let prev_old = self.line_string(self.cursor_y - 1);
            let current_old = self.line_string(self.cursor_y);
            self.history
                .record(Operation::Change, self.cursor_y - 1, prev_old);
            self.history
                .record(Operation::Delete, self.cursor_y, current_old);

            self.semantic_tokens.remove(self.cursor_y);
            let current_line = self.lines.remove(self.cursor_y);
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
            self.lines[self.cursor_y].extend(current_line);
            self.modified = true;
        }
    }

    pub fn insert_newline(&mut self) {
        // 1. Определяем отступ текущей строки до переноса
        let old_line = self.line_string(self.cursor_y);
        let current_line = &self.lines[self.cursor_y];
        let mut indent_chars = Vec::new();

        for &c in current_line.iter() {
            if c == ' ' || c == '\t' {
                indent_chars.push(c);
            } else {
                break;
            }
        }

        // 2. Разделяем строку по позиции курсора
        let current_line_mut = &mut self.lines[self.cursor_y];
        let mut next_line = current_line_mut.split_off(self.cursor_x);
        let mut indent_len = cmp::min(indent_chars.len(), self.cursor_x);
        // 3. Добавляем скопированный отступ в начало новой строки
        if self.has_indent && self.cursor_x > indent_chars.len() {
            indent_chars.extend(next_line);
            next_line = indent_chars;
        } else {
            indent_len = 0
        }

        // 4. Вставляем новую строку и пустой слот для токенов подсветки
        let new_line_str: String = next_line.iter().collect();
        self.history
            .record(Operation::Change, self.cursor_y, old_line);
        self.lines.insert(self.cursor_y + 1, next_line);
        self.history
            .record(Operation::Insert, self.cursor_y + 1, new_line_str);
        self.semantic_tokens.insert(self.cursor_y + 1, Vec::new());

        // 5. Переводим курсор на новую строку, смещая его на длину автоотступа
        self.cursor_y += 1;
        self.cursor_x = indent_len;
        self.modified = true;
    }

    pub fn toggle_indent(&mut self) {
        self.has_indent = !self.has_indent;
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let old = self.line_string(self.cursor_y);
        self.history.record(Operation::Change, self.cursor_y, old);
        for c in s.chars() {
            self.lines[self.cursor_y].insert(self.cursor_x, c);
            self.cursor_x += 1;
        }
        self.modified = true;
    }

    pub fn move_word_left(&mut self) {
        let current_line_idx = self.cursor_y;
        let line = &self.lines[current_line_idx];
        let tokens = self.semantic_tokens.get(current_line_idx);

        // Сценарий А: Если у нас есть семантические токены от LSP/Лексера
        if let Some(toks) = tokens {
            if !toks.is_empty() {
                let mut target_x = 0;
                for token in toks.iter().rev() {
                    if token.start_col < self.cursor_x {
                        target_x = token.start_col;
                        break;
                    }
                }
                if target_x == 0 && self.cursor_x == 0 && self.cursor_y > 0 {
                    self.cursor_y -= 1;
                    self.cursor_x = self.lines[self.cursor_y].len();
                } else {
                    self.cursor_x = target_x;
                }
                return;
            }
        }

        // Сценарий Б: Фоллбэк — прыгаем по пробелам
        if self.cursor_x > 0 {
            let mut x = self.cursor_x - 1;
            while x > 0 && (line[x] == ' ' || line[x] == '\t') {
                x -= 1;
            }
            while x > 0 && line[x] != ' ' && line[x] != '\t' {
                x -= 1;
            }
            if x > 0 || (line[x] == ' ' || line[x] == '\t') {
                self.cursor_x = x + 1;
            } else {
                self.cursor_x = 0;
            }
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
        }
    }

    pub fn move_word_right(&mut self) {
        let current_line_idx = self.cursor_y;
        let line = &self.lines[current_line_idx];
        let tokens = self.semantic_tokens.get(current_line_idx);

        // Сценарий А: Перемещение по LSP-токенам
        if let Some(toks) = tokens {
            if !toks.is_empty() {
                let mut target_x = line.len();
                for token in toks {
                    if token.start_col > self.cursor_x {
                        target_x = token.start_col;
                        break;
                    }
                }
                if target_x == line.len()
                    && self.cursor_x == line.len()
                    && self.cursor_y < self.lines.len() - 1
                {
                    self.cursor_y += 1;
                    self.cursor_x = 0;
                } else {
                    self.cursor_x = target_x;
                }
                return;
            }
        }

        // Сценарий Б: Фоллбэк по пробелам
        if self.cursor_x < line.len() {
            let mut x = self.cursor_x;
            while x < line.len() && line[x] != ' ' && line[x] != '\t' {
                x += 1;
            }
            while x < line.len() && (line[x] == ' ' || line[x] == '\t') {
                x += 1;
            }
            self.cursor_x = x;
        } else if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    pub fn get_block(&self) -> (usize, usize, usize, usize) {
        if let Some(sel) = self.selection {
            let (p1_y, p1_x) = (sel.start_y, sel.start_x);
            let (p2_y, p2_x) = (self.cursor_y, self.cursor_x);
            if p1_y < p2_y || (p1_y == p2_y && p1_x <= p2_x) {
                return (p1_y, p1_x, p2_y, p2_x);
            } else {
                return (p2_y, p2_x, p1_y, p1_x);
            }
        }
        let line = &self.lines[self.cursor_y];
        (self.cursor_y, 0, self.cursor_y, line.len())
    }

    /// Возвращает выделенный текст в виде String, либо текущую строку
    pub fn get_selected_text(&self) -> String {
        let (start_y, start_x, end_y, end_x) = self.get_block();

        let mut result = String::new();
        for y in start_y..=end_y {
            if y >= self.lines.len() {
                break;
            }
            let line = &self.lines[y];
            let s_x = if y == start_y {
                start_x.min(line.len())
            } else {
                0
            };
            let e_x = if y == end_y {
                end_x.min(line.len())
            } else {
                line.len()
            };

            let segment: String = line[s_x..e_x].iter().collect();
            result.push_str(&segment);
            if y < end_y {
                result.push('\n');
            }
        }

        result
    }

    /// Удаляет выделенный текст, если он есть
    pub fn delete_selection(&mut self) {
        let (start_y, start_x, end_y, end_x) = self.get_block();
        let had_content = start_y != end_y || start_x < end_x;

        if start_y == end_y {
            if start_y < self.lines.len() {
                if start_x < end_x {
                    let old = self.line_string(start_y);
                    self.history.record(Operation::Change, start_y, old);
                }
                self.lines[start_y].drain(start_x..end_x);
                self.semantic_tokens[start_y].clear();
            }
        } else {
            let mut remaining_suffix = Vec::new();
            if end_y < self.lines.len() {
                remaining_suffix = self.lines[end_y][end_x..].to_vec();
            }

            // Сначала удаление строк ниже, затем изменение стартовой строки.
            // Удалённые строки пишем в обратном порядке (снизу вверх), чтобы
            // undo восстанавливал их сверху вниз с правильными индексами.
            let old_start = self.line_string(start_y);
            //self.history.record(Operation::Change, start_y, old_start);
            let last_deleted = end_y.min(self.lines.len().saturating_sub(1));
            for y in ((start_y + 1)..=last_deleted).rev() {
                let old = self.line_string(y);
                self.history.record(Operation::Delete, y, old);
            }
            self.history.record(Operation::Change, start_y, old_start);

            if start_y + 1 < self.lines.len() {
                let drain_end = (end_y + 1).min(self.lines.len());
                self.lines.drain((start_y + 1)..drain_end);
                self.semantic_tokens.drain((start_y + 1)..drain_end);
            }
            if start_y < self.lines.len() {
                self.lines[start_y].truncate(start_x);
                self.lines[start_y].extend(remaining_suffix);
                self.semantic_tokens[start_y].clear();
            }
        }

        self.cursor_y = start_y;
        self.cursor_x = start_x;
        self.selection = None;
        if had_content {
            self.modified = true;
        }
    }

    /// Копирование выделенного фрагмента (или текущей строки) в ClipboardManager
    pub fn copy_to_clipboard(&self) {
        let text_to_copy = self.get_selected_text();

        if !text_to_copy.is_empty() {
            ClipboardManager::global().set_text(text_to_copy);
        }
    }

    /// Вырезание (копирование + удаление)
    pub fn cut_to_clipboard(&mut self) {
        self.copy_to_clipboard();
        self.delete_selection();
    }

    /// Вставка текста из ClipboardManager в позицию курсора
    pub fn paste_from_clipboard(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
        }

        let text = ClipboardManager::global().get_text();
        if text.is_empty() {
            return;
        }

        let start_y = self.cursor_y;
        let old_line = self.line_string(start_y);
        let insert_lines: Vec<&str> = text.split('\n').collect();

        if insert_lines.len() == 1 {
            let chars: Vec<char> = insert_lines[0].chars().collect();
            let len = chars.len();
            self.history.record(Operation::Change, start_y, old_line);
            self.lines[start_y].splice(self.cursor_x..self.cursor_x, chars);
            self.semantic_tokens[start_y].clear();
            self.cursor_x += len;
        } else {
            let mut inserted_strings: Vec<String> = Vec::new();

            let current_line = &mut self.lines[start_y];
            let tail: Vec<char> = current_line.split_off(self.cursor_x);

            current_line.extend(insert_lines[0].chars());
            self.semantic_tokens[start_y].clear();

            let mut insert_idx = start_y + 1;

            for &middle_line in &insert_lines[1..insert_lines.len() - 1] {
                let chars: Vec<char> = middle_line.chars().collect();
                inserted_strings.push(middle_line.to_string());
                self.lines.insert(insert_idx, chars);
                self.semantic_tokens.insert(insert_idx, Vec::new());
                insert_idx += 1;
            }

            let mut last_inserted_line: Vec<char> = insert_lines.last().unwrap().chars().collect();
            let last_line_len = last_inserted_line.len();
            last_inserted_line.extend(tail);
            inserted_strings.push(last_inserted_line.iter().collect());

            self.lines.insert(insert_idx, last_inserted_line);
            self.semantic_tokens.insert(insert_idx, Vec::new());

            self.cursor_y = insert_idx;
            self.cursor_x = last_line_len;

            self.history.record(Operation::Change, start_y, old_line);
            for (offset, line) in inserted_strings.into_iter().enumerate() {
                self.history
                    .record(Operation::Insert, start_y + 1 + offset, line);
            }
        }

        self.modified = true;
    }

    pub fn begin_selection(&mut self) {
        if self.selection.is_none() {
            self.selection = Some(Selection {
                start_x: self.cursor_x,
                start_y: self.cursor_y,
            });
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn set_cursor(&mut self, y: usize, x: usize) {
        self.cursor_y = y.min(self.lines.len().saturating_sub(1));
        self.cursor_x = x.min(self.lines[self.cursor_y].len());
        self.selection = None;
    }

    /// Ищет `query` начиная с текущей строки с переносом в начало
    /// и перемещает курсор на найденный фрагмент.
    pub fn find_in_editor(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }

        let needle: Vec<char> = query.chars().collect();
        let total = self.lines.len();
        if total == 0 {
            return;
        }

        for i in 0..total {
            let y = (self.cursor_y + i) % total;
            if let Some(x) = self.lines[y]
                .windows(needle.len())
                .position(|window| window == needle.as_slice())
            {
                self.set_cursor(y, x);
                return;
            }
        }
    }
}

impl TextInteractive for BaseEditor {
    fn title(&self) -> String {
        if self.modified {
            format!("{}*", self.filename)
        } else {
            self.filename.clone()
        }
    }

    fn lines(&self) -> &Vec<Vec<char>> {
        &self.lines
    }

    fn get_all_text(&self) -> String {
        self.get_all_text()
    }

    fn load_lines(&mut self, lines: Vec<Vec<char>>) {
        self.load_lines(lines)
    }

    fn mark_saved(&mut self) {
        self.mark_saved()
    }

    fn update_window_size(&mut self) -> io::Result<()> {
        self.update_window_size()
    }

    fn scroll(&mut self) {
        self.scroll()
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        self.draw(stdout)
    }

    fn update_semantic_tokens(&mut self, tokens: Vec<Vec<RenderToken>>) {
        self.update_semantic_tokens(tokens)
    }

    fn set_diagnostics(&mut self, errors: usize, warnings: usize, diagnostics: Vec<LspDiagnostic>) {
        self.set_diagnostics(errors, warnings, diagnostics)
    }

    fn diagnostics(&self) -> &[LspDiagnostic] {
        &self.diagnostics
    }

    fn diagnostics_icon_at(&self, x: u16, y: u16) -> bool {
        self.diagnostics_icon_at(x, y)
    }

    fn jump_to_diagnostic(&mut self, index: usize) {
        self.jump_to_diagnostic(index)
    }

    fn cursor_x(&self) -> usize {
        self.cursor_x
    }

    fn cursor_y(&self) -> usize {
        self.cursor_y
    }

    fn row_offset(&self) -> usize {
        self.row_offset
    }

    fn col_offset(&self) -> usize {
        self.col_offset
    }

    fn set_cursor(&mut self, y: usize, x: usize) {
        self.set_cursor(y, x)
    }

    fn find_in_editor(&mut self, query: &str) {
        self.find_in_editor(query)
    }

    fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    fn begin_selection(&mut self) {
        self.begin_selection()
    }

    fn clear_selection(&mut self) {
        self.clear_selection()
    }

    fn click_mouse(&mut self, x: u16, y: u16) {
        self.click_mouse(x, y)
    }

    fn move_up(&mut self) {
        self.move_up()
    }

    fn move_down(&mut self) {
        self.move_down()
    }

    fn move_left(&mut self) {
        self.move_left()
    }

    fn move_right(&mut self) {
        self.move_right()
    }

    fn page_up(&mut self) {
        self.page_up()
    }

    fn page_down(&mut self) {
        self.page_down()
    }

    fn move_to_line_start(&mut self) {
        self.move_to_line_start()
    }

    fn move_to_line_end(&mut self) {
        self.move_to_line_end()
    }

    fn move_word_left(&mut self) {
        self.move_word_left()
    }

    fn move_word_right(&mut self) {
        self.move_word_right()
    }

    fn insert_char(&mut self, c: char) {
        self.insert_char(c)
    }

    fn insert_str(&mut self, s: &str) {
        self.insert_str(s)
    }

    fn insert_newline(&mut self) {
        self.insert_newline()
    }

    fn delete_char(&mut self) {
        self.delete_char()
    }

    fn delete_char_at_cursor(&mut self) {
        self.delete_char_at_cursor()
    }

    fn delete_selection(&mut self) {
        self.delete_selection()
    }

    fn clear_current_line(&mut self) {
        self.clear_current_line()
    }

    fn toggle_indent(&mut self) {
        self.toggle_indent()
    }

    fn copy_to_clipboard(&self) {
        self.copy_to_clipboard()
    }

    fn cut_to_clipboard(&mut self) {
        self.cut_to_clipboard()
    }

    fn paste_from_clipboard(&mut self) {
        self.paste_from_clipboard()
    }

    fn undo(&mut self) {
        self.undo()
    }

    fn redo(&mut self) {
        self.redo()
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn quit(&mut self) {
        self.quit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with(lines: &[&str]) -> BaseEditor {
        let mut editor = BaseEditor::new("test".to_string(), Vec::new());
        editor.load_lines(lines.iter().map(|s| s.chars().collect()).collect());
        editor
    }

    fn text(editor: &BaseEditor) -> Vec<String> {
        editor
            .lines
            .iter()
            .map(|line| line.iter().collect())
            .collect()
    }

    #[test]
    fn diagnostics_icon_opens_and_jumps_to_line() {
        let mut editor = editor_with(&["one", "two", "three"]);
        assert!(!editor.diagnostics_icon_at(10, 0));

        editor.set_diagnostics(
            1,
            1,
            vec![
                LspDiagnostic {
                    start_line: 2,
                    start_col: 2,
                    end_line: 2,
                    end_col: 3,
                    severity: 1,
                    message: "error".to_string(),
                },
                LspDiagnostic {
                    start_line: 1,
                    start_col: 0,
                    end_line: 1,
                    end_col: 1,
                    severity: 2,
                    message: "warning".to_string(),
                },
            ],
        );

        let start = editor.header_left().chars().count() as u16;
        assert!(editor.diagnostics_icon_at(start, 0));
        assert!(editor.diagnostics_icon_at(start + 4, 0));
        assert!(!editor.diagnostics_icon_at(start + 5, 0));
        assert!(!editor.diagnostics_icon_at(start, 1));

        editor.jump_to_diagnostic(0);
        assert_eq!((editor.cursor_y, editor.cursor_x), (2, 2));

        editor.jump_to_diagnostic(1);
        assert_eq!((editor.cursor_y, editor.cursor_x), (1, 0));

        editor.jump_to_diagnostic(99);
        assert_eq!((editor.cursor_y, editor.cursor_x), (1, 0));
    }

    #[test]
    fn matching_brackets_finds_pairs_across_lines() {
        let mut editor = editor_with(&["fn main() {", "    let x = [1, 2];", "}"]);

        editor.set_cursor(0, 10);
        assert_eq!(editor.matching_brackets(), Some(((0, 10), (2, 0))));

        editor.set_cursor(1, 12);
        assert_eq!(editor.matching_brackets(), Some(((1, 12), (1, 17))));

        editor.set_cursor(2, 0);
        assert_eq!(editor.matching_brackets(), Some(((2, 0), (0, 10))));

        editor.set_cursor(0, 0);
        assert_eq!(editor.matching_brackets(), None);
    }

    #[test]
    fn title_marks_modified_file_with_asterisk() {
        let mut editor = editor_with(&["hello"]);
        assert_eq!(editor.title(), "test");

        editor.insert_char('x');
        assert_eq!(editor.title(), "test*");

        editor.mark_saved();
        assert_eq!(editor.title(), "test");

        editor.undo();
        assert_eq!(editor.title(), "test*");
    }

    #[test]
    fn find_in_editor_wraps_and_moves_cursor() {
        let mut editor = editor_with(&["hello", "world", "search me"]);

        editor.find_in_editor("world");
        assert_eq!((editor.cursor_y, editor.cursor_x), (1, 0));

        editor.find_in_editor("me");
        assert_eq!((editor.cursor_y, editor.cursor_x), (2, 7));

        // Начиная с последней строки поиск переносится в начало.
        editor.find_in_editor("hello");
        assert_eq!((editor.cursor_y, editor.cursor_x), (0, 0));

        // Если фрагмент не найден, курсор остаётся на месте.
        editor.find_in_editor("missing");
        assert_eq!((editor.cursor_y, editor.cursor_x), (0, 0));
    }

    #[test]
    fn multiline_selection_undo_redo() {
        let mut editor = editor_with(&["AA", "BB", "CC", "DD", "EE"]);
        editor.selection = Some(Selection {
            start_x: 1,
            start_y: 1,
        });
        editor.cursor_y = 3;
        editor.cursor_x = 1;

        editor.delete_selection();
        assert_eq!(text(&editor), vec!["AA", "BD", "EE"]);

        editor.undo();
        editor.undo();
        editor.undo();
        assert_eq!(text(&editor), vec!["AA", "BB", "CC", "DD", "EE"]);

        editor.redo();
        editor.redo();
        editor.redo();
        assert_eq!(text(&editor), vec!["AA", "BD", "EE"]);
    }
}

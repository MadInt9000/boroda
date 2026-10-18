// Добавьте в начало src/editor.rs
use crate::lsp_adapter::LspAdapter;
use crate::tokens::RenderToken; // Подключаем единую структуру
use crossterm::{
    cursor,
    event::{Event, KeyCode},
    terminal,
    QueueableCommand,
};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

pub struct LspDiagnostic {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub message: String,
}

pub struct Editor {
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
    pub clipboard: Vec<char>,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<LspDiagnostic>,
    // ИНДЕКС: Каждому элементу lines соответствует свой вектор токенов
    pub semantic_tokens: Vec<Vec<RenderToken>>,
    // ДОБАВЛЯЕМ СВОЙСТВО: Адаптер конкретного файла
    pub lsp_adapter: Option<LspAdapter>,
}

impl Editor {
    pub fn new(filename: String) -> Self {
        let mut editor = Self {
            filename,
            lines: Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_offset: 0,
            col_offset: 0,
            screen_cols: 0,
            screen_rows: 0,
            has_indent: true,
            should_quit: false,
            clipboard: Vec::new(),
            error_count: 0,
            warning_count: 0,
            diagnostics: Vec::new(),
            semantic_tokens: Vec::new(),
            lsp_adapter: None, 
        };
        editor.reload_file();
        editor
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

    pub fn reload_file(&mut self) {
        let mut new_lines = Vec::new();
        let abs_path = Path::new(&self.filename);
        
        if abs_path.exists() {
            if let Ok(file) = File::open(&abs_path) {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(ln) = line {
                        new_lines.push(ln.chars().collect());
                    }
                }
            }
        }
        if new_lines.is_empty() {
            new_lines.push(Vec::new());
        }
        self.lines = new_lines;
        
        // Синхронизируем размер индекса токенов с количеством строк
        self.semantic_tokens = vec![Vec::new(); self.lines.len()];
        
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_offset = 0;
        self.col_offset = 0;
        self.diagnostics.clear();
    }

    pub async fn search_substring(&mut self) {
        use futures_util::StreamExt;
        let mut query = String::new();
        let prompt_prefix = "Search: ";
        let mut reader = crossterm::event::EventStream::new();
        
        loop {
            let _ = io::stdout().queue(cursor::MoveTo(0, self.screen_rows - 1));
            let current_view = format!("{}{}", prompt_prefix, query);
            let fill = " ".repeat((self.screen_cols as usize).saturating_sub(current_view.len()));
            print!("\x1b[7m{}{}\x1b[m", current_view, fill);
            let _ = io::stdout().flush();

            if let Some(Ok(Event::Key(key))) = reader.next().await {
                match key.code {
                    KeyCode::Enter => break,
                    KeyCode::Esc => return,
                    KeyCode::Backspace => { query.pop(); }
                    KeyCode::Char(c) => { query.push(c); }
                    _ => {}
                }
            }
        }

        if query.is_empty() { return; }

        let query_chars: Vec<char> = query.chars().collect();
        let start_y = self.cursor_y;

        for i in 0..self.lines.len() {
            let check_y = (start_y + i) % self.lines.len();
            let line = &self.lines[check_y];
            
            if let Some(pos) = line.windows(query_chars.len()).position(|w| w == query_chars) {
                self.cursor_y = check_y;
                self.cursor_x = pos;
                return;
            }
        }
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
            if row > diag.start_line && row < diag.end_line { return true; }
            if row == diag.start_line && row < diag.end_line && col >= diag.start_col { return true; }
            if row == diag.end_line && row > diag.start_line && col < diag.end_col { return true; }
            if row == diag.start_line && row == diag.end_line {
                if col >= diag.start_col && col < diag.end_col { return true; }
                if let Some(line) = self.lines.get(row) {
                    if diag.start_col >= line.len() && col == line.len().saturating_sub(1) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn get_token_color(&self, token_type: u32) -> &'static str {
        match token_type {
            1 | 10 => "\x1b[36m", // Ключевые слова -> Бирюзовый
            2 | 3 | 4 => "\x1b[32m", // Структуры, Типы -> Зеленый
            6 => "\x1b[34m", // Функции -> Синий
            5 | 8 => "\x1b[33m", // Строки -> Желтый
            7 | 9  => "\x1b[35m", // Хз -> Фиолетовый
            0 => "\x1b[90m", // Комментарии -> Серый
            _ => "\x1b[0m",
        }
    }

    fn draw_line_with_lsp_syntax(&self, file_row: usize, render_len: usize) {
        let line = &self.lines[file_row];
        let row_tokens = self.semantic_tokens.get(file_row);
    
        // Индекс текущего проверяемого токена в строке
        let mut token_idx = 0;

        for col_idx in 0..render_len {
            let file_col = col_idx + self.col_offset;
            if file_col >= line.len() { break; }
            let c = line[file_col];

            // 1. Проверяем диагностику ошибок (она приоритетнее подсветки)
            let is_err = self.is_error_char(file_row, file_col);
        
            if is_err {
                print!("\x1b[1;31m{}\x1b[0m", c);
            } else {
                let mut color = "\x1b[0m";

                if let Some(tokens) = row_tokens {
                    // Продвигаем указатель токенов вперед, если текущий символ 
                    // уже находится правее текущего токена
                    while token_idx < tokens.len() && file_col >= tokens[token_idx].start_col + tokens[token_idx].len {
                        token_idx += 1;
                    }

                    // Если нашли токен, в границы которого попадает наш символ — берем его цвет
                    if token_idx < tokens.len() {
                        let token = &tokens[token_idx];
                        if file_col >= token.start_col && file_col < token.start_col + token.len {
                            color = self.get_token_color(token.token_type);
                        }
                    }
                }
                print!("{}{}\x1b[0m", color, c);
            }
        }
    }

    //pub fn warning_function(&self) {
    // just do nothing
    //}

    pub fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        let _ = stdout.queue(cursor::Hide);
        //stdout.queue(terminal::Clear(ClearType::All))?;

        // 1. Статус-бар
        stdout.queue(cursor::MoveTo(0, 0))?;

        // Левая часть статус-бара: имя файла, количество строк и позиция курсора
        let status_left = format!(
            " File: {} | Lines: {} | Pos: {},{} | ", 
            self.filename, self.lines.len(), self.cursor_y + 1, self.cursor_x + 1
        );
        
        // Правая часть статус-бара: выводим значки ошибок и предупреждений друг за другом
        let mut status_right = String::new();
        for _ in 0..self.error_count {
            status_right.push_str("🛑");
        }
        for _ in 0..self.warning_count {
            status_right.push_str("⚠️");
        }
        // Добавим финальный пробел для отступа справа, если значки есть
//        if !status_right.is_empty() {
//            status_right.push(' ');
//        }

        // Вычисляем свободное пространство между левой и правой частью
        // Учитываем, что эмодзи занимают больше байт в UTF-8, но на экране отображаются как 2 знакоместа.
        // Для безопасного вычисления используем длину левой части и фактическое количество иконок.
        let right_visual_len = (self.error_count * 2) + (self.warning_count * 2) + if status_right.is_empty() { 0 } else { 1 };
        let total_used = status_left.len() + right_visual_len;
        
        let fill_len = (self.screen_cols as usize).saturating_sub(total_used);
        let fill = " ".repeat(fill_len);

        // Собираем итоговую строку для вывода в инвертированных цветах терминала
        let full_status = format!("{}{}{}", status_left, status_right, fill);
        
        // Отрезаем строку под ширину экрана на случай очень узкого окна
        let render_status = if full_status.chars().count() > self.screen_cols as usize {
            full_status.chars().take(self.screen_cols as usize).collect::<String>()
        } else {
            full_status
        };
        print!("\x1b[7m{}\x1b[m", render_status);

        // 2. Отображение текста с подсветкой синтаксиса (Очистка через ANSI)
        let text_rows = self.screen_rows.saturating_sub(2) as usize;
        for i in 0..text_rows {
            let file_row = i + self.row_offset;
            stdout.queue(cursor::MoveTo(0, i as u16 + 1))?;
            
            if file_row < self.lines.len() {
                let line = &self.lines[file_row];
                if line.len() > self.col_offset {
                    let render_len = (line.len() - self.col_offset).min(self.screen_cols as usize);
                    self.draw_line_with_lsp_syntax(file_row, render_len);
                }
                // \x1b[K очищает всё от конца выведенного текста до правого края экрана.
                // Код сработает корректно и для заполненной строки, и для абсолютно пустой.
                print!("\x1b[K"); 
            } else {
                // Если мы вышли за пределы файла (пустые строки внизу экрана) — чистим всю строку
                print!("\x1b[K");
            }
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
            "F1 TogIdt|F2 Save|F3 Search|F4 New|F5 Clear|F6 Info|F7 Cut|F8 Paste|F9 Begin|F10 Quit|F11 ExSave|F12 Reload".to_string()
        });

        let render_hints = if footer_text.len() > self.screen_cols as usize {
            &footer_text[..self.screen_cols as usize]
        } else {
            &footer_text
        };
        let fill_hint = " ".repeat((self.screen_cols as usize).saturating_sub(render_hints.len()));
        print!("\x1b[7m{}{}\x1b[m", render_hints, fill_hint);

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

    pub fn save_file(&self) -> io::Result<()> {
        let mut file = File::create(&self.filename)?;
        for line in &self.lines {
            let s: String = line.iter().collect();
            writeln!(file, "{}", s)?;
        }
        Ok(())
    }

    pub fn new_file(&mut self) {
        self.lines = vec![Vec::new()];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.filename = "untitled.txt".to_string();
        self.error_count = 0;
        self.warning_count = 0;
        self.diagnostics.clear();
    }

    pub fn clear_current_line(&mut self) {
        if !self.lines.is_empty() {
            self.lines[self.cursor_y].clear();
            self.semantic_tokens[self.cursor_y].clear(); // Очищаем токены строки
            self.cursor_x = 0;
        }
    }

    pub fn cut_line(&mut self) {
        if self.lines.len() > 1 {
            self.clipboard = self.lines.remove(self.cursor_y);
            self.semantic_tokens.remove(self.cursor_y); // Удаляем строку из индекса подсветки
            if self.cursor_y >= self.lines.len() {
                self.cursor_y = self.lines.len() - 1;
            }
        } else {
            self.clipboard = self.lines[self.cursor_y].clone();
            self.semantic_tokens[self.cursor_y].clear();
            self.lines[self.cursor_y].clear();
        }
            self.cursor_x = 0;
    }

    pub fn paste_line(&mut self) {
        if !self.clipboard.is_empty() {
            self.lines.insert(self.cursor_y, self.clipboard.clone());
            self.semantic_tokens.insert(self.cursor_y, Vec::new()); // Вставляем пустой слот для
            self.cursor_x = 0;
        }
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
        self.lines[self.cursor_y].insert(self.cursor_x, c);
        self.cursor_x += 1;
    }
    
    pub fn delete_char_at_cursor(&mut self) {
        if self.cursor_x < self.lines[self.cursor_y].len() {
            self.lines[self.cursor_y].remove(self.cursor_x);
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor_x > 0 {
            self.lines[self.cursor_y].remove(self.cursor_x - 1);
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.semantic_tokens.remove(self.cursor_y); // Удаляем индекс удаляемой строки
            let current_line = self.lines.remove(self.cursor_y);
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
            self.lines[self.cursor_y].extend(current_line);
        }
    }

    pub fn insert_newline(&mut self) {
        // 1. Определяем отступ текущей строки до переноса
        let current_line = &self.lines[self.cursor_y];
        let mut indent_chars = Vec::new();
        
        // Копируем все пробелы и символы табуляции с начала строки
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
        let mut indent_len = indent_chars.len();
        // 3. Добавляем скопированный отступ в начало новой строки
        if self.has_indent {
            indent_chars.extend(next_line);
            next_line = indent_chars;
        } else {
            indent_len = 0
        }
        
        // 4. Вставляем новую строку и пустой слот для токенов подсветки
        self.lines.insert(self.cursor_y + 1, next_line);
        self.semantic_tokens.insert(self.cursor_y + 1, Vec::new());

        // 5. Переводим курсор на новую строку, смещая его на длину автоотступа
        self.cursor_y += 1;
        self.cursor_x = indent_len;
    }
    
    pub fn toggle_indent(&mut self) {
        self.has_indent = !self.has_indent;
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            self.lines[self.cursor_y].insert(self.cursor_x, c);
            self.cursor_x += 1;
        }
    }

    pub fn move_word_left(&mut self) {
        let current_line_idx = self.cursor_y;
        let line = &self.lines[current_line_idx];
        let tokens = self.semantic_tokens.get(current_line_idx);

        // Сценарий А: Если у нас есть семантические токены от LSP/Лексера
        if let Some(toks) = tokens {
            if !toks.is_empty() {
                // Ищем ближайший токен, который начинается левее курсора
                let mut target_x = 0;
                for token in toks.iter().rev() {
                    if token.start_col < self.cursor_x {
                        target_x = token.start_col;
                        break;
                    }
                }
                // Если мы уже стояли в начале первого токена строки, прыгаем на конец предыдущей строки
                if target_x == 0 && self.cursor_x == 0 && self.cursor_y > 0 {
                    self.cursor_y -= 1;
                    self.cursor_x = self.lines[self.cursor_y].len();
                } else {
                    self.cursor_x = target_x;
                }
                return;
            }
        }

        // Сценарий Б: Фоллбэк (если токенов нет) — прыгаем по пробелам
        if self.cursor_x > 0 {
            let mut x = self.cursor_x - 1;
            // Пропускаем пробелы слева
            while x > 0 && (line[x] == ' ' || line[x] == '\t') { x -= 1; }
            // Ищем начало слова
            while x > 0 && line[x] != ' ' && line[x] != '\t' { x -= 1; }
            if x > 0 || (line[x] == ' ' || line[x] == '\t') { self.cursor_x = x + 1; } 
            else { self.cursor_x = 0; }
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
                // Если правее токенов нет, переходим в начало следующей строки
                if target_x == line.len() && self.cursor_x == line.len() && self.cursor_y < self.lines.len() - 1 {
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
            // Пропускаем текущие не-пробелы
            while x < line.len() && line[x] != ' ' && line[x] != '\t' { x += 1; }
            // Пропускаем последующие пробелы
            while x < line.len() && (line[x] == ' ' || line[x] == '\t') { x += 1; }
            self.cursor_x = x;
        } else if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

} 

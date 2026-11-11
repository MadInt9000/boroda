use crate::traits::{ModalInteractive, ModalResult};
use crossterm::{cursor, style::Print, terminal, QueueableCommand};
use std::io::{self, Write};

const BORDER_H: char = '─';
const MAX_MODAL_WIDTH: usize = 70;

/// Общее состояние закрытия модалки.
#[derive(Default)]
struct ModalState {
    result: Option<ModalResult>,
}

impl ModalState {
    fn reset(&mut self) {
        self.result = None;
    }

    fn submit(&mut self) {
        self.result = Some(ModalResult::Submit);
    }

    fn cancel(&mut self) {
        self.result = Some(ModalResult::Cancel);
    }

    fn take(&mut self) -> Option<ModalResult> {
        self.result.take()
    }
}

/// Прямоугольник, в котором рисуется модальное окно.
struct Rect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    inner_width: u16,
}

impl Rect {
    fn new(cols: u16, rows: u16, width: usize, height: u16) -> Self {
        let cols = cols.max(4) as usize;
        let rows = rows.max(3) as usize;
        let max_width = cols.saturating_sub(2).min(MAX_MODAL_WIDTH).max(4);
        let width = width.max(4).min(max_width);
        let height = height.max(3).min(rows.saturating_sub(1).max(3) as u16);
        let x = ((cols - width) / 2) as u16;
        let y = (rows.saturating_sub(height as usize) / 2) as u16;

        Self {
            x,
            y,
            width: width as u16,
            height,
            inner_width: width.saturating_sub(2) as u16,
        }
    }
}

fn draw_frame(
    stdout: &mut io::Stdout,
    rect: &Rect,
    title: &str,
    blank_content: bool,
) -> io::Result<()> {
    let inner = rect.inner_width as usize;
    let title = format!(" {} ", title);
    let title: String = title.chars().take(inner).collect();

    let mut top = String::from("\x1b[42;30m┌");
    top.push_str(&title);
    top.push_str(&BORDER_H.to_string().repeat(inner - title.chars().count()));
    top.push_str("┐\x1b[m");

    stdout.queue(cursor::MoveTo(rect.x, rect.y))?;
    stdout.queue(Print(top))?;

    if blank_content {
        let blank = " ".repeat(inner);
        for row in 1..rect.height.saturating_sub(1) {
            stdout.queue(cursor::MoveTo(rect.x, rect.y + row))?;
            stdout.queue(Print(format!("\x1b[42;30m│{}│\x1b[m", blank)))?;
        }
    }

    let bottom = format!("\x1b[42;30m└{}┘\x1b[m", BORDER_H.to_string().repeat(inner));
    stdout.queue(cursor::MoveTo(
        rect.x,
        rect.y + rect.height.saturating_sub(1),
    ))?;
    stdout.queue(Print(bottom))?;
    Ok(())
}

fn write_content(
    stdout: &mut io::Stdout,
    x: u16,
    y: u16,
    inner: u16,
    text: &str,
    highlighted: bool,
) -> io::Result<()> {
    let clipped: String = text.chars().take(inner as usize).collect();
    let pad = (inner as usize).saturating_sub(clipped.chars().count());
    let content = if highlighted {
        format!(
            "\x1b[42;30m│\x1b[40;37m{}{}\x1b[42;30m│\x1b[m",
            clipped,
            " ".repeat(pad)
        )
    } else {
        format!("\x1b[42;30m│{}{}│\x1b[m", clipped, " ".repeat(pad))
    };

    stdout.queue(cursor::MoveTo(x, y))?;
    stdout.queue(Print(content))?;
    Ok(())
}

/* ------------------------------------------------------------------------- */
/* ModalTextInput                                                            */
/* ------------------------------------------------------------------------- */

/// Модальное окно однострочного ввода (поиск, команды и т.п.).
pub struct ModalTextInput {
    prompt: String,
    value: Vec<char>,
    cursor: usize,
    state: ModalState,
}

impl ModalTextInput {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            value: Vec::new(),
            cursor: 0,
            state: ModalState::default(),
        }
    }
}

impl ModalInteractive for ModalTextInput {
    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
        }
    }

    fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    fn move_to_end(&mut self) {
        self.cursor = self.value.len();
    }

    fn character_input(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn control_char(&mut self, c: char) {
        match c.to_ascii_lowercase() {
            'a' => self.cursor = 0,
            'e' => self.cursor = self.value.len(),
            'u' => {
                self.value.drain(..self.cursor);
                self.cursor = 0;
            }
            _ => {}
        }
    }

    fn delete_backward(&mut self) {
        if self.cursor > 0 {
            self.value.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    fn delete_forward(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    fn submit(&mut self) {
        self.state.submit();
    }

    fn close_modal(&mut self) {
        self.state.cancel();
    }

    fn open(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.state.reset();
    }

    fn text(&self) -> Option<String> {
        Some(self.value.iter().collect())
    }

    fn take_result(&mut self) -> Option<ModalResult> {
        self.state.take()
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        let (cols, rows) = terminal::size()?;
        let prompt_len = self.prompt.chars().count();
        // Ширину не привязываем к текущему значению, иначе окно будет
        // «дрожать» при вводе и оставлять следы от предыдущей отрисовки.
        let width = prompt_len.max(10) + 44;
        let rect = Rect::new(cols, rows, width, 3);

        draw_frame(stdout, &rect, &self.prompt, false)?;

        let value: String = self.value.iter().collect();
        let text = format!("> {}", value);
        write_content(stdout, rect.x, rect.y + 1, rect.inner_width, &text, false)?;

        let cursor_x = (rect.x + 3 + self.cursor as u16).min(rect.x + rect.width - 2);
        stdout.queue(cursor::MoveTo(cursor_x, rect.y + 1))?;
        stdout.queue(cursor::Show)?;
        stdout.flush()?;
        Ok(())
    }
}

/* ------------------------------------------------------------------------- */
/* ModalConfirm                                                              */
/* ------------------------------------------------------------------------- */

struct ConfirmLayout {
    rect: Rect,
    yes_x: u16,
    no_x: u16,
    button_y: u16,
}

/// Модальное окно подтверждения с кнопками Yes / No.
pub struct ModalConfirm {
    message: String,
    yes_selected: bool,
    state: ModalState,
}

impl ModalConfirm {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            yes_selected: true,
            state: ModalState::default(),
        }
    }

    fn layout(&self) -> io::Result<ConfirmLayout> {
        let (cols, rows) = terminal::size()?;
        let width = (self.message.chars().count() + 4).max(24);
        let rect = Rect::new(cols, rows, width, 5);

        // "[ Yes ]" = 7, промежуток = 2, "[ No ]" = 6
        let buttons_width = 7 + 2 + 6;
        let offset = (rect.inner_width as usize).saturating_sub(buttons_width) / 2;
        let yes_x = rect.x + 1 + offset as u16;
        let no_x = yes_x + 7 + 2;

        Ok(ConfirmLayout {
            button_y: rect.y + 3,
            rect,
            yes_x,
            no_x,
        })
    }
}

impl ModalInteractive for ModalConfirm {
    fn move_left(&mut self) {
        self.yes_selected = true;
    }

    fn move_right(&mut self) {
        self.yes_selected = false;
    }

    fn tab(&mut self) {
        self.yes_selected = !self.yes_selected;
    }

    fn character_input(&mut self, c: char) {
        if c.eq_ignore_ascii_case(&'y') {
            self.yes_selected = true;
            self.state.submit();
        } else if c.eq_ignore_ascii_case(&'n') {
            self.yes_selected = false;
            self.state.submit();
        }
    }

    fn submit(&mut self) {
        self.state.submit();
    }

    fn close_modal(&mut self) {
        self.state.cancel();
    }

    fn mouse_click(&mut self, x: u16, y: u16) {
        let Ok(layout) = self.layout() else {
            return;
        };
        if y != layout.button_y {
            return;
        }
        if x >= layout.yes_x && x < layout.yes_x + 7 {
            self.yes_selected = true;
            self.state.submit();
        } else if x >= layout.no_x && x < layout.no_x + 6 {
            self.yes_selected = false;
            self.state.submit();
        }
    }

    fn open(&mut self) {
        self.yes_selected = true;
        self.state.reset();
    }

    fn confirmed(&self) -> bool {
        self.yes_selected
    }

    fn take_result(&mut self) -> Option<ModalResult> {
        self.state.take()
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        let layout = self.layout()?;
        let rect = &layout.rect;

        draw_frame(stdout, rect, "Confirm", true)?;

        let inner = rect.inner_width as usize;
        let message: String = self.message.chars().take(inner).collect();
        let left = (inner.saturating_sub(message.chars().count())) / 2;
        write_content(
            stdout,
            rect.x,
            rect.y + 1,
            rect.inner_width,
            &format!("{}{}", " ".repeat(left), message),
            false,
        )?;

        let yes_label = "[ Yes ]";
        let no_label = "[ No ]";

        stdout.queue(cursor::MoveTo(layout.yes_x, layout.button_y))?;
        if self.yes_selected {
            stdout.queue(Print(format!("\x1b[7m{}\x1b[m", yes_label)))?;
        } else {
            stdout.queue(Print(format!("\x1b[42m{}\x1b[m", yes_label)))?;
        }

        stdout.queue(cursor::MoveTo(layout.no_x, layout.button_y))?;
        if self.yes_selected {
            stdout.queue(Print(format!("\x1b[42m{}\x1b[m", no_label)))?;
        } else {
            stdout.queue(Print(format!("\x1b[7m{}\x1b[m", no_label)))?;
        }

        stdout.queue(cursor::Hide)?;
        stdout.flush()?;
        Ok(())
    }
}

/* ------------------------------------------------------------------------- */
/* ModalMenu                                                                 */
/* ------------------------------------------------------------------------- */

struct MenuLayout {
    rect: Rect,
    list_y: u16,
    visible: usize,
}

/// Модальное меню со списком элементов (переключение файлов, команды).
pub struct ModalMenu {
    title: String,
    items: Vec<String>,
    selected: usize,
    scroll: usize,
    state: ModalState,
}

impl ModalMenu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            selected: 0,
            scroll: 0,
            state: ModalState::default(),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }

    fn layout(&self) -> io::Result<MenuLayout> {
        let (cols, rows) = terminal::size()?;
        let title_len = self.title.chars().count();
        let max_item = self
            .items
            .iter()
            .map(|item| item.chars().count())
            .max()
            .unwrap_or(0);
        let width = max_item.max(title_len) + 6;

        let max_visible = (rows as usize).saturating_sub(4).max(1);
        let visible = self.items.len().clamp(1, max_visible);
        let rect = Rect::new(cols, rows, width, (visible + 2) as u16);

        Ok(MenuLayout {
            list_y: rect.y + 1,
            visible,
            rect,
        })
    }
}

impl ModalInteractive for ModalMenu {
    fn move_up(&mut self) {
        self.move_selection(-1);
    }

    fn move_down(&mut self) {
        self.move_selection(1);
    }

    fn page_up(&mut self) {
        self.move_selection(-10);
    }

    fn page_down(&mut self) {
        self.move_selection(10);
    }

    fn move_to_start(&mut self) {
        if !self.items.is_empty() {
            self.selected = 0;
        }
    }

    fn move_to_end(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.items.len() - 1;
        }
    }

    fn tab(&mut self) {
        self.move_selection(1);
    }

    fn character_input(&mut self, c: char) {
        match c {
            'j' => self.move_selection(1),
            'k' => self.move_selection(-1),
            _ => {}
        }
    }

    fn submit(&mut self) {
        if !self.items.is_empty() {
            self.state.submit();
        }
    }

    fn close_modal(&mut self) {
        self.state.cancel();
    }

    fn mouse_click(&mut self, _x: u16, y: u16) {
        let Ok(layout) = self.layout() else {
            return;
        };
        if y < layout.list_y {
            return;
        }
        let row = (y - layout.list_y) as usize;
        if row >= layout.visible {
            return;
        }
        let index = self.scroll + row;
        if index < self.items.len() {
            self.selected = index;
            self.state.submit();
        }
    }

    fn mouse_scroll(&mut self, up: bool) {
        self.move_selection(if up { -1 } else { 1 });
    }

    fn open(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.state.reset();
    }

    fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.selected = 0;
        self.scroll = 0;
    }

    fn set_selected(&mut self, index: usize) {
        self.selected = if self.items.is_empty() {
            0
        } else {
            index.min(self.items.len() - 1)
        };
    }

    fn selected(&self) -> Option<usize> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.selected)
        }
    }

    fn take_result(&mut self) -> Option<ModalResult> {
        self.state.take()
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> io::Result<()> {
        let layout = self.layout()?;

        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + layout.visible {
            self.scroll = self.selected + 1 - layout.visible;
        }

        draw_frame(stdout, &layout.rect, &self.title, false)?;

        for row in 0..layout.visible {
            let index = self.scroll + row;
            let y = layout.list_y + row as u16;
            let item = self.items.get(index).map(String::as_str).unwrap_or("");
            write_content(
                stdout,
                layout.rect.x,
                y,
                layout.rect.inner_width,
                item,
                index == self.selected,
            )?;
        }

        stdout.queue(cursor::Hide)?;
        stdout.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::ModalInteractive;

    #[test]
    fn text_input_edits_and_submits() {
        let mut modal = ModalTextInput::new("Search");
        modal.open();

        modal.character_input('a');
        modal.character_input('b');
        assert_eq!(modal.text(), Some("ab".to_string()));

        modal.delete_backward();
        assert_eq!(modal.text(), Some("a".to_string()));

        modal.submit();
        assert_eq!(modal.take_result(), Some(ModalResult::Submit));
        assert_eq!(modal.take_result(), None);

        modal.close_modal();
        assert_eq!(modal.take_result(), Some(ModalResult::Cancel));
    }

    #[test]
    fn confirm_toggles_and_returns_choice() {
        let mut modal = ModalConfirm::new("Quit?");
        modal.open();
        assert!(modal.confirmed());

        modal.tab();
        assert!(!modal.confirmed());

        modal.tab();
        assert!(modal.confirmed());

        modal.submit();
        assert_eq!(modal.take_result(), Some(ModalResult::Submit));
        assert!(modal.confirmed());
    }

    #[test]
    fn menu_moves_and_submits_selection() {
        let mut modal = ModalMenu::new("Files");
        modal.open();
        modal.set_items(vec!["a.rs".to_string(), "b.rs".to_string()]);
        modal.set_selected(0);
        assert_eq!(modal.selected(), Some(0));

        modal.move_down();
        assert_eq!(modal.selected(), Some(1));

        modal.move_down();
        assert_eq!(modal.selected(), Some(0));

        modal.submit();
        assert_eq!(modal.take_result(), Some(ModalResult::Submit));

        modal.close_modal();
        assert_eq!(modal.take_result(), Some(ModalResult::Cancel));
    }
}

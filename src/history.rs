/// Операция, которую нужно применить к строке при undo/redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Строка была вставлена.
    Insert,
    /// Строка была удалена.
    Delete,
    /// Строка была изменена.
    Change,
}

/// Одна запись построчной истории.
///
/// Для `Change` в `string` хранится предыдущее содержимое строки
/// (чтобы undo мог его восстановить), для `Insert` — вставленная строка,
/// для `Delete` — удалённая строка.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub operation: Operation,
    pub number: usize,
    pub string: String,
}

/// Построчная история изменений одного редактора.
pub struct History {
    pub historic_line: Vec<HistoryEntry>,
    redo_line: Vec<HistoryEntry>,
    /// Разрешать ли объединение следующей записи с последней.
    /// После undo/redo объединение запрещено, чтобы новая правка
    /// не «приклеивалась» к уже отменённому/повторённому шагу.
    mergeable: bool,
}

impl History {
    pub fn new() -> Self {
        Self {
            historic_line: Vec::new(),
            redo_line: Vec::new(),
            mergeable: false,
        }
    }

    /// Записывает изменение строки.
    ///
    /// Последовательные изменения одной и той же строки объединяются
    /// в одну запись: сохраняется самое раннее (исходное) содержимое.
    pub fn record(&mut self, operation: Operation, number: usize, string: String) {
        if operation == Operation::Change && self.mergeable {
            if let Some(last) = self.historic_line.last_mut() {
                if last.operation == Operation::Change && last.number == number {
                    self.redo_line.clear();
                    return;
                }
            }
        }

        self.historic_line.push(HistoryEntry {
            operation,
            number,
            string,
        });
        self.redo_line.clear();
        self.mergeable = true;
    }

    pub fn clear(&mut self) {
        self.historic_line.clear();
        self.redo_line.clear();
        self.mergeable = false;
    }

    /// Отменяет последнее изменение. Возвращает `true`, если что-то отменили.
    pub fn undo(&mut self, lines: &mut Vec<Vec<char>>, cursor_y: &mut usize) -> bool {
        let Some(entry) = self.historic_line.pop() else {
            return false;
        };

        self.mergeable = false;
        self.redo_line.push(Self::apply_undo(&entry, lines));
        *cursor_y = entry.number;
        true
    }

    /// Повторяет последнее отменённое изменение.
    pub fn redo(&mut self, lines: &mut Vec<Vec<char>>, cursor_y: &mut usize) -> bool {
        let Some(entry) = self.redo_line.pop() else {
            return false;
        };

        self.mergeable = false;
        self.historic_line.push(Self::apply_redo(&entry, lines));
        *cursor_y = entry.number;
        true
    }

    fn apply_undo(entry: &HistoryEntry, lines: &mut Vec<Vec<char>>) -> HistoryEntry {
        match entry.operation {
            Operation::Insert => {
                // Отменяем вставку — удаляем добавленную строку.
                if entry.number < lines.len() {
                    lines.remove(entry.number);
                }
                entry.clone()
            }
            Operation::Delete => {
                // Отменяем удаление — возвращаем строку на место.
                let idx = entry.number.min(lines.len());
                lines.insert(idx, entry.string.chars().collect());
                entry.clone()
            }
            Operation::Change => {
                // Отменяем изменение — восстанавливаем старое содержимое.
                Self::swap_line(entry, lines)
            }
        }
    }

    fn apply_redo(entry: &HistoryEntry, lines: &mut Vec<Vec<char>>) -> HistoryEntry {
        match entry.operation {
            Operation::Insert => {
                // Повторяем вставку.
                let idx = entry.number.min(lines.len());
                lines.insert(idx, entry.string.chars().collect());
                entry.clone()
            }
            Operation::Delete => {
                // Повторяем удаление.
                if entry.number < lines.len() {
                    lines.remove(entry.number);
                }
                entry.clone()
            }
            Operation::Change => {
                // Повторяем изменение — ставим новое содержимое.
                Self::swap_line(entry, lines)
            }
        }
    }

    /// Меняет строку на `entry.string` и возвращает обратную запись
    /// с прежним содержимым строки.
    fn swap_line(entry: &HistoryEntry, lines: &mut Vec<Vec<char>>) -> HistoryEntry {
        if entry.number < lines.len() {
            let current: String = lines[entry.number].iter().collect();
            lines[entry.number] = entry.string.chars().collect();
            HistoryEntry {
                operation: Operation::Change,
                number: entry.number,
                string: current,
            }
        } else {
            lines.push(entry.string.chars().collect());
            HistoryEntry {
                operation: Operation::Change,
                number: entry.number,
                string: String::new(),
            }
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<Vec<char>> {
        items.iter().map(|s| s.chars().collect()).collect()
    }

    fn text(lines: &[Vec<char>]) -> Vec<String> {
        lines.iter().map(|line| line.iter().collect()).collect()
    }

    #[test]
    fn change_is_merged_and_undone_as_one_step() {
        let mut history = History::new();
        let mut buffer = lines(&[""]);
        let mut cursor_y = 0;

        // Пользователь напечатал "a", затем "b".
        history.record(Operation::Change, 0, "".to_string());
        buffer[0] = "a".chars().collect();

        history.record(Operation::Change, 0, "a".to_string());
        buffer[0] = "ab".chars().collect();

        assert_eq!(history.historic_line.len(), 1);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec![""]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["ab"]);
    }

    #[test]
    fn insert_line_undo_redo() {
        let mut history = History::new();
        let mut buffer = lines(&["a"]);
        let mut cursor_y = 0;

        history.record(Operation::Insert, 1, "b".to_string());
        buffer.insert(1, "b".chars().collect());

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a", "b"]);
    }

    #[test]
    fn delete_line_undo_redo() {
        let mut history = History::new();
        let mut buffer = lines(&["a", "b"]);
        let mut cursor_y = 0;

        history.record(Operation::Delete, 1, "b".to_string());
        buffer.remove(1);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a", "b"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a"]);
    }

    #[test]
    fn multiline_delete_undo_redo() {
        let mut history = History::new();
        let mut buffer = lines(&["A", "B", "C", "D"]);
        let mut cursor_y = 0;

        // Выделили с середины B до середины C и удалили.
        history.record(Operation::Change, 1, "B".to_string());
        history.record(Operation::Delete, 2, "C".to_string());
        buffer[1] = "B'".chars().collect();
        buffer.remove(2);
        assert_eq!(text(&buffer), vec!["A", "B'", "D"]);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["A", "B'", "C", "D"]);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["A", "B", "C", "D"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["A", "B'", "C", "D"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["A", "B'", "D"]);
    }

    #[test]
    fn new_line_undo_redo() {
        let mut history = History::new();
        let mut buffer = lines(&["ab"]);
        let mut cursor_y = 0;

        history.record(Operation::Change, 0, "ab".to_string());
        buffer[0] = "a".chars().collect();
        history.record(Operation::Insert, 1, "b".to_string());
        buffer.insert(1, "b".chars().collect());
        assert_eq!(text(&buffer), vec!["a", "b"]);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["ab"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a", "b"]);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut history = History::new();
        let mut buffer = lines(&[""]);
        let mut cursor_y = 0;

        history.record(Operation::Change, 0, "".to_string());
        buffer[0] = "a".chars().collect();

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec![""]);

        history.record(Operation::Change, 0, "".to_string());
        buffer[0] = "x".chars().collect();

        assert!(!history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["x"]);
    }

    #[test]
    fn multi_line_paste_undo_redo() {
        let mut history = History::new();
        let mut buffer = lines(&["aX"]);
        let mut cursor_y = 0;

        // Вставка "b\nc\nd" после символа 'a'.
        history.record(Operation::Change, 0, "aX".to_string());
        buffer[0] = "ab".chars().collect();
        history.record(Operation::Insert, 1, "c".to_string());
        buffer.insert(1, "c".chars().collect());
        history.record(Operation::Insert, 2, "dX".to_string());
        buffer.insert(2, "dX".chars().collect());
        assert_eq!(text(&buffer), vec!["ab", "c", "dX"]);

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["aX"]);

        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["ab", "c", "dX"]);
    }

    #[test]
    fn edit_after_redo_starts_new_step() {
        let mut history = History::new();
        let mut buffer = lines(&[""]);
        let mut cursor_y = 0;

        history.record(Operation::Change, 0, "".to_string());
        buffer[0] = "a".chars().collect();

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert!(history.redo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a"]);

        history.record(Operation::Change, 0, "a".to_string());
        buffer[0] = "ab".chars().collect();

        assert!(history.undo(&mut buffer, &mut cursor_y));
        assert_eq!(text(&buffer), vec!["a"]);
    }
}

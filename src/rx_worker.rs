use crate::lsp::LspClient;
use crate::lsp_adapter::LspParseResult;
use crate::traits::{FileOperations, TextInteractive};
use serde_json::Value;
use std::io;

pub struct RxWorker;

impl RxWorker {
    /// Обрабатывает одно сообщение из мультиплексного потока LSP-серверов
    /// через уже собранные представления редакторов.
    ///
    /// Если определение указывает на неоткрытый файл, возвращает цель,
    /// чтобы вызывающий код загрузил файл и пересобрал представления.
    pub fn handle_message(
        lsp_msg: &Value,
        text_editors: &mut [&mut dyn TextInteractive],
        file_editors: &mut [&mut dyn FileOperations],
        current_editor_idx: &mut usize,
        lsp_pool: &[LspClient],
        stdout: &mut io::Stdout,
    ) -> io::Result<Option<(String, usize, usize)>> {
        let mut state_changed = false;
        let mut goto_target = None;

        for (text, file) in text_editors.iter_mut().zip(file_editors.iter_mut()) {
            // 1. Предварительно собираем атомарные данные из editor
            let text_content = text.get_all_text();
            let total_lines = text.lines().len();

            // 2. Спокойно вызываем парсинг — никакого конфликта заимствований
            let parse_result = if let Some(lsp) = file.lsp_adapter() {
                lsp.parse_lsp_message(lsp_pool, lsp_msg, &text_content, total_lines)
            } else {
                None
            };

            if let Some(result) = parse_result {
                match result {
                    LspParseResult::Tokens(ready_tokens) => {
                        text.update_semantic_tokens(ready_tokens);
                        state_changed = true;
                    }
                    LspParseResult::Diagnostics {
                        errors,
                        warnings,
                        diagnostics,
                    } => {
                        text.set_diagnostics(errors, warnings, diagnostics);
                        state_changed = true;
                    }
                    LspParseResult::Definition {
                        abs_path,
                        line,
                        col,
                    } => {
                        // Проверяем, что этот ответ пришел именно на запрос данного редактора
                        goto_target = Some((abs_path, line, col));
                    }
                }
            }
        }

        // Если сервер вернул координаты определения, перемещаемся туда
        if let Some((target_path, target_line, target_col)) = goto_target {
            // Шаг А: Ищем, открыт ли этот файл уже в каком-то редакторе
            let found_idx = file_editors.iter().position(|file| {
                // Нормализуем пути перед сравнением (на всякий случай)
                if let (Ok(p1), Ok(p2)) = (
                    std::fs::canonicalize(file.filename()),
                    std::fs::canonicalize(&target_path),
                ) {
                    p1 == p2
                } else {
                    file.filename() == target_path
                }
            });

            if let Some(target_idx) = found_idx {
                // Шаг Б: Файл уже открыт — переключаемся и сдвигаем курсор
                *current_editor_idx = target_idx;
                text_editors[target_idx].set_cursor(target_line, target_col);
                state_changed = true;
            } else {
                // Шаг В: Файл не открыт — просим вызывающий код загрузить его
                return Ok(Some((target_path, target_line, target_col)));
            }
        }

        if state_changed {
            let editor = &mut *text_editors[*current_editor_idx];
            editor.update_window_size()?;
            editor.scroll();
            editor.draw(stdout)?;
        }

        Ok(None)
    }
}

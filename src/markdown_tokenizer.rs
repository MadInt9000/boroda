use crate::tokens::RenderToken;
use regex::Regex;

pub struct MarkdownTokenizer;

impl MarkdownTokenizer {
    /// Построчный лексер Markdown с поддержкой многострочных блоков кода и инлайн-стилей
    pub fn tokenize_text(lines: &[Vec<char>]) -> Vec<Vec<RenderToken>> {
        let mut file_tokens = vec![Vec::new(); lines.len()];
        let mut in_code_block = false;

        // Ленивая инициализация регулярных выражений для инлайн-элементов
        // 3 = Литералы / Подчеркнутый (используем для курсива/ссылок)
        // 4 = Жирный
        // 0 = Серый (используем для инлайн-кода)
        // Замените старые регулярки в src/markdown_tokenizer.rs на эти:
        let bold_stars = Regex::new(r"\*\*(.*?)\*\*").unwrap();
        let bold_underscores = Regex::new(r"__(.*?)__").unwrap();

        let italic_stars = Regex::new(r"\*(.*?)\*").unwrap();
        let italic_underscores = Regex::new(r"_(.*?)_").unwrap();

        let inline_code_regex = Regex::new(r"`([^`]+)`").unwrap();
        let link_regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();

        for (row_idx, line) in lines.iter().enumerate() {
            let mut tokens = Vec::new();
            let line_str: String = line.iter().collect();

            // 1. Обработка многострочных блоков кода (```)
            if line_str.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 0, // Серый цвет для границ блока кода
                });
                file_tokens[row_idx] = tokens;
                continue;
            }

            if in_code_block {
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 0, // Весь текст внутри блока кода — серый
                });
                file_tokens[row_idx] = tokens;
                continue;
            }

            // 2. Обработка заголовков (# Heading)
            if line_str.trim_start().starts_with('#') {
                // Считаем количество решеток для определения уровня (опционально)
                let hashes_len = line_str.len() - line_str.trim_start_matches('#').len();
                if line_str.chars().nth(hashes_len) == Some(' ') {
                    tokens.push(RenderToken {
                        start_col: 0,
                        len: line.len(),
                        token_type: 1, // Бирюзовый / Яркий цвет для заголовков
                    });
                    file_tokens[row_idx] = tokens;
                    continue;
                }
            }

            // 3. Обработка элементов списков и цитат (> , * , - , 1.)
            let trimmed = line_str.trim_start();
            if trimmed.starts_with("> ") || trimmed.starts_with("* ") || trimmed.starts_with("- ") || 
               (trimmed.len() > 2 && trimmed.chars().next().unwrap().is_ascii_digit() && trimmed.contains(". ")) {
                let indent = line_str.len() - trimmed.len();
                tokens.push(RenderToken {
                    start_col: indent,
                    len: if trimmed.starts_with("> ") || trimmed.starts_with("* ") || trimmed.starts_with("- ") { 1 } else { trimmed.find('.').unwrap() + 1 },
                    token_type: 6, // Синий цвет для маркеров списков
                });
            }

            // 4. Посимвольный маппинг для инлайн-стилей (Жирный, Курсив, Код, Ссылки)
            // Создаем маску стилей для строки (0 — обычный текст)
            let mut style_mask = vec![99u32; line.len()];

            // Функция для применения стиля на диапазон байт (переводим в индексы символов)
            let mut apply_style = |regex: &Regex, token_type: u32, marker_len: usize| {
                for mat in regex.find_iter(&line_str) {
                    let start_byte = mat.start();
                    let end_byte = mat.end();
                    
                    // Переводим байтовые смещения regex в индексы символов char
                    let start_char = line_str[..start_byte].chars().count();
                    let end_char = line_str[..end_byte].chars().count();


                    // Красим только текст внутри (например, для **жирного** пропускаем сами **)
                    //let marker_len = if regex.as_str().contains(r"\*\*") { 2 } else { 1 };
                    for i in (start_char + marker_len)..(end_char - marker_len) {
                        if i < style_mask.len() { style_mask[i] = token_type; }
                    }
                }
            };

            // Применяем регулярные выражения поочередно от менее приоритетных к более приоритетным
            apply_style(&link_regex, 9, 0);        // Ссылки -> Подчеркнутый (3)

            apply_style(&italic_stars, 3, 1);       // Курсив * -> 
            apply_style(&italic_underscores, 9, 1); // Курсив _ -> Подчеркнутый (3)

            apply_style(&bold_stars, 4, 2);         // Жирный ** -> Жирный (4)
            apply_style(&bold_underscores, 10, 2);   // Подчеркнутый жирный __ -> Жирный (11)

            apply_style(&inline_code_regex, 6, 0); // Инлайн-код -> Синий (6)

            // Собираем непрерывные токены из маски стилей
            let mut col = 0;
            while col < line.len() {
                let current_style = style_mask[col];
                if current_style != 99 {
                    let start = col;
                    while col < line.len() && style_mask[col] == current_style {
                        col += 1;
                    }
                    tokens.push(RenderToken {
                        start_col: start,
                        len: col - start,
                        token_type: current_style,
                    });
                } else {
                    col += 1;
                }
            }

            // Сортируем токены по начальной колонке, чтобы draw_line их корректно обработал
            tokens.sort_by_key(|t| t.start_col);
            file_tokens[row_idx].extend(tokens);
        }

        file_tokens
    }
}


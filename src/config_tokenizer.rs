use crate::tokens::RenderToken;
use regex::Regex;

pub struct ConfigTokenizer;

impl ConfigTokenizer {
    /// Универсальный лексер для конфигурационных файлов (.json, .toml, .ini) без look-ahead
    pub fn tokenize_text(lines: &[Vec<char>]) -> Vec<Vec<RenderToken>> {
        let mut file_tokens = vec![Vec::new(); lines.len()];

        // 1. Компилируем регулярные выражения
        let comment_ini_toml = Regex::new(r"[#;].*").unwrap(); 
        let string_double = Regex::new(r#""[^"\\]*(?:\\.[^"\\]*)*""#).unwrap(); 
        let string_single = Regex::new(r#"'[^'\\]*(?:\\.[^'\\]*)*'"#).unwrap(); 
        
        let section_regex = Regex::new(r"^[ \t]*\[+[^\]]+\]+").unwrap();
        
        // ИСПРАВЛЕНО: Захватываем ключ в группу 1, а разделитель (:=) со сопутствующими пробелами включаем в общий поиск
        let key_regex = Regex::new(r#"(?:^[ \t]*|[ \t,]+)((?:"[^"]+"?|'[^']+'?|[a-zA-Z0-9_\-\.]+))\s*[:=]"#).unwrap();
        
        let number_regex = Regex::new(r"\b\-?\d+(?:\.\d+)?\b").unwrap();
        let literal_regex = Regex::new(r"\b(true|false|null|inf|nan)\b").unwrap();

        for (row_idx, line) in lines.iter().enumerate() {
            let mut tokens = Vec::new();
            let line_str: String = line.iter().collect();

            // 2. Маска стилей (99 = обычный текст/знаки препинания)
            let mut style_mask = vec![99u32; line.len()];

            // Замыкание для заполнения маски
            let mut fill_mask = |start_byte: usize, end_byte: usize, token_type: u32| {
                let start_char = line_str[..start_byte].chars().count();
                let end_char = line_str[..end_byte].chars().count();
                for i in start_char..end_char {
                    if i < style_mask.len() {
                        style_mask[i] = token_type;
                    }
                }
            };

            // 3. Применяем регулярные выражения от низкого приоритета к высокому
            
            // Числа (1 = Бирюзовый)
            for mat in number_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 1);
            }

            // Булевы значения и null (1 = Бирюзовый)
            for mat in literal_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 2);
            }

            // ИСПРАВЛЕНО: Ключи/Параметры (2 = Зеленый) — красим только группу 1
            for cap in key_regex.captures_iter(&line_str) {
                if let Some(mat) = cap.get(1) {
                    fill_mask(mat.start(), mat.end(), 3);
                }
            }

            // Строки в двойных кавычках (5 = Желтый)
            for mat in string_double.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 14);
            }

            // Строки в одинарных кавычках (5 = Желтый)
            for mat in string_single.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 14);
            }

            // Заголовки секций (6 = Синий)
            for mat in section_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 6);
            }

            // Комментарии (0 = Серый)
            for mat in comment_ini_toml.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 0);
            }

            // 4. Сборка непрерывных токенов из маски
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

            tokens.sort_by_key(|t| t.start_col);
            file_tokens[row_idx] = tokens;
        }

        file_tokens
    }
}


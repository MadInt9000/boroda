use crate::tokens::RenderToken;
use regex::Regex;

pub struct PythonTokenizer;

impl PythonTokenizer {
    /// Построчный лексер Python на регулярных выражениях с использованием замыкания
    pub fn tokenize_text(lines: &[Vec<char>]) -> Vec<Vec<RenderToken>> {
        let mut file_tokens = vec![Vec::new(); lines.len()];

        // 1. Компилируем регулярные выражения
        let comment_regex = Regex::new(r"#.*").unwrap();
        let string_double = Regex::new(r#""[^"\\]*(?:\\.[^"\\]*)*""#).unwrap();
        let string_single = Regex::new(r#"'[^'\\]*(?:\\.[^'\\]*)*'"#).unwrap();
        
        let keywords_regex = Regex::new(
            r"\b(def|class|return|if|elif|else|import|from|as|for|in|while|try|except|finally|with|assert|pass|break|continue|lambda|None|True|False|and|or|not|is|global|nonlocal)\b"
        ).unwrap();

        let class_name_regex = Regex::new(r"\b[A-Z][a-zA-Z0-9_]*\b").unwrap();
        let function_call_regex = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();
        let number_regex = Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap();

        let mut in_multiline = false;

        for (row_idx, line) in lines.iter().enumerate() {
            let mut tokens = Vec::new();
            let line_str: String = line.iter().collect();

            // 2. Обработка многострочных блоков (Docstrings: """ или ''')
            let doc_count = line_str.matches("\"\"\"").count() + line_str.matches("'''").count();
            if doc_count > 0 {
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 14,
                });
                file_tokens[row_idx] = tokens;
                if doc_count == 1 { 
                    in_multiline = !in_multiline; 
                }
                continue;
            }

            if in_multiline {
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 14,
                });
                file_tokens[row_idx] = tokens;
                continue;
            }

            // 3. Маска стилей для текущей строки (99 = обычный текст)
            let mut style_mask = vec![99u32; line.len()];

            // Универсальное замыкание для заполнения маски
            let mut fill_mask = |start_byte: usize, end_byte: usize, token_type: u32| {
                let start_char = line_str[..start_byte].chars().count();
                let end_char = line_str[..end_byte].chars().count();
                for i in start_char..end_char {
                    if i < style_mask.len() {
                        style_mask[i] = token_type;
                    }
                }
            };

            // 4. Применяем регулярные выражения с использованием замыкания
            
            // Числа (1 = Бирюзовый)
            for mat in number_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 1);
            }

            // Вызовы функций (8 = ярко-жёлтый) — берем только первую группу (имя функции)
            for cap in function_call_regex.captures_iter(&line_str) {
                if let Some(mat) = cap.get(1) {
                    fill_mask(mat.start(), mat.end(), 8);
                }
            }

            // Классы (3 = Подчеркнутый зеленый)
            for mat in class_name_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 3);
            }

            // Ключевые слова (6 = Синий)
            for mat in keywords_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 6);
            }

            // Строки "" (5 = Желтый)
            for mat in string_double.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 14);
            }

            // Строки '' (5 = Желтый)
            for mat in string_single.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 14);
            }

            // Комментарии (0 = Серый)
            for mat in comment_regex.find_iter(&line_str) {
                fill_mask(mat.start(), mat.end(), 0);
            }

            // 5. Сборка токенов из маски стилей
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


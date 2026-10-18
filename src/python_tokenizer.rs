use crate::tokens::RenderToken;

pub struct PythonTokenizer;

impl PythonTokenizer {
    /// Метод разбирает весь файл построчно и возвращает индекс токенов для каждой строки
    pub fn tokenize_file(lines: &[Vec<char>]) -> Vec<Vec<RenderToken>> {
        let mut file_tokens = vec![Vec::new(); lines.len()];

        let py_keywords = [
            "def", "class", "return", "if", "elif", "else", "import", "from", "as", 
            "for", "in", "while", "try", "except", "finally", "with", "assert", 
            "pass", "break", "continue", "lambda", "None", "True", "False", 
            "and", "or", "not", "is", "global", "nonlocal"
        ];

        for (row_idx, line) in lines.iter().enumerate() {
            let mut col = 0;
            let mut tokens = Vec::new();

            let mut in_string = false;
            let mut string_char = '"';

            while col < line.len() {
                let c = line[col];

                // 1. Обработка комментариев (#)
                if !in_string && c == '#' {
                    tokens.push(RenderToken {
                        start_col: col,
                        len: line.len() - col,
                        token_type: 0, // 0 = Комментарий (Серый)
                    });
                    break; // Комментарий идет до конца строки
                }

                // 2. Обработка строковых литералов
                if in_string {
                    let start_str_col = col;
                    while col < line.len() {
                        if line[col] == string_char {
                            col += 1;
                            in_string = false;
                            break;
                        }
                        col += 1;
                    }
                    tokens.push(RenderToken {
                        start_col: start_str_col,
                        len: col - start_str_col,
                        token_type: 5, // 5 = Строка
                    });
                    continue;
                }

                if c == '"' || c == '\'' {
                    in_string = true;
                    string_char = c;
                    tokens.push(RenderToken {
                        start_col: col,
                        len: 1,
                        token_type: 5,
                    });
                    col += 1;
                    continue;
                }

                // 3. Обработка слов (Буквенно-цифровые токены)
                if c.is_alphabetic() || c == '_' {
                    let start_word_col = col;
                    let mut word = String::new();
                    
                    while col < line.len() && (line[col].is_alphanumeric() || line[col] == '_') {
                        word.push(line[col]);
                        col += 1;
                    }

                    let token_type = if py_keywords.contains(&word.as_str()) {
                        6 // 6 = Ключевое слово (Синий)
                    } else if word.chars().next().unwrap().is_uppercase() {
                        3 // 3 = Класс / Тип (Зеленый)
                    } else if col < line.len() && line[col] == '(' {
                        2 // 2 = Метод / Функция (Зеленый)
                    } else {
                        99 // Обычный текст, не красим
                    };

                    if token_type != 99 {
                        tokens.push(RenderToken {
                            start_col: start_word_col,
                            len: col - start_word_col,
                            token_type,
                        });
                    }
                    continue;
                }

                col += 1;
            }
            file_tokens[row_idx] = tokens;
        }

        file_tokens
    }
}

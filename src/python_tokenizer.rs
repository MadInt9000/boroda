use crate::tokens::RenderToken;

pub struct PythonTokenizer;

impl PythonTokenizer {
    /// Построчный лексер Python с поддержкой многострочных блоков и подсветки чисел
    pub fn tokenize_file(lines: &[Vec<char>]) -> Vec<Vec<RenderToken>> {
        let mut file_tokens = vec![Vec::new(); lines.len()];

        let py_keywords = [
            "def", "class", "return", "if", "elif", "else", "import", "from", "as", 
            "for", "in", "while", "try", "except", "finally", "with", "assert", 
            "pass", "break", "continue", "lambda", "None", "True", "False", 
            "and", "or", "not", "is", "global", "nonlocal"
        ];

        let mut in_multiline = false;

        for (row_idx, line) in lines.iter().enumerate() {
            let mut col = 0;
            let mut tokens = Vec::new();

            // Переводим вектор char в String для быстрой проверки наличия тегов
            let line_str: String = line.iter().collect();

            // 1. Проверяем наличие тройных кавычек в строке. Если они есть — инвертируем флаг.
            if line_str.contains("\"\"\"") || line_str.contains("'''") {
                in_multiline = !in_multiline;
                
                // Красим текущую строку (где блок открылся или закрылся) целиком
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 5, // 5 = Цвет строки/Docstring (Желтый)
                });
                file_tokens[row_idx] = tokens;
                continue; // Переходим к следующей строке файла
            }

            // 2. Если флаг установлен (мы внутри тела многострочного блока) — просто красим и скипаем
            if in_multiline {
                tokens.push(RenderToken {
                    start_col: 0,
                    len: line.len(),
                    token_type: 5,
                });
                file_tokens[row_idx] = tokens;
                continue;
            }

            // 3. Стандартный посимвольный разбор обычного Python-кода
            let mut in_string = false;
            let mut string_char = '"';

            while col < line.len() {
                let c = line[col];

                // Обработка обычных однострочных строк
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
                        token_type: 5,
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

                // Однострочные комментарии (#)
                if c == '#' {
                    tokens.push(RenderToken {
                        start_col: col,
                        len: line.len() - col,
                        token_type: 0,
                    });
                    break;
                }

                // Буквенно-цифровые токены (Ключевые слова, Классы, Функции, Переменные с цифрами)
                // Сначала обрабатываем слова, чтобы переменные вроде "var2" красились целиком как текст
                if c.is_alphabetic() || c == '_' {
                    let start_word_col = col;
                    let mut word = String::new();

                    while col < line.len() && (line[col].is_alphanumeric() || line[col] == '_') {
                        word.push(line[col]);
                        col += 1;
                    }

                    let token_type = if py_keywords.contains(&word.as_str()) {
                        6 // Ключевое слово (Синий)
                    } else if word.chars().next().unwrap().is_uppercase() {
                        3 // Класс
                    } else if col < line.len() && line[col] == '(' {
                        2 // Функция
                    } else {
                        99 // Обычная переменная или текст
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

                // 4. Подсветка обособленных чисел (Важно: идет ПОСЛЕ проверки на буквы)
                // Токены вроде "42", "3.14" распарсятся здесь, а цифры внутри "x12" уйдут выше в текст
                if c.is_ascii_digit() {
                    let start_num_col = col;
                    
                    // Считываем цифры и точку (для float-чисел)
                    while col < line.len() && (line[col].is_ascii_digit() || line[col] == '.') {
                        col += 1;
                    }

                    tokens.push(RenderToken {
                        start_col: start_num_col,
                        len: col - start_num_col,
                        token_type: 1, // 1 = Бирюзовый цвет (в get_token_color перенаправляет на ключевые слова)
                    });
                    continue;
                }

                col += 1;
            }
            file_tokens[row_idx] = tokens;
        }

        file_tokens
    }
}


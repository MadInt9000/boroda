use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Configuration {
    pub file_types: HashMap<String, String>,
    pub lsp_servers: HashMap<String, LspServerConfig>,
    pub color_palettes: HashMap<String, HashMap<String, String>>,
    pub default_palette: HashMap<String, String>,
}

impl Default for Configuration {
    fn default() -> Self {
        let mut file_types = HashMap::new();
        file_types.insert("py".to_string(), "python".to_string());
        file_types.insert("rs".to_string(), "rust".to_string());
        file_types.insert("php".to_string(), "php".to_string());

        let mut lsp_servers = HashMap::new();
        lsp_servers.insert(
            "python".to_string(),
            LspServerConfig { command: "pyright-langserver".to_string(), args: vec!["--stdio".to_string()] },
        );
        lsp_servers.insert(
            "rust".to_string(),
            LspServerConfig { command: "rust-analyzer".to_string(), args: vec![] },
        );
        lsp_servers.insert(
            "php".to_string(),
            LspServerConfig { command: "intelephense".to_string(), args: vec!["--stdio".to_string()] },
        );

        let mut default_palette = HashMap::new();
        default_palette.insert("0".to_string(), "\x1b[90m".to_string());    // Комментарии
        default_palette.insert("1".to_string(), "\x1b[0;36m".to_string());   // Ключевые слова / Числа
        default_palette.insert("2".to_string(), "\x1b[0;32m".to_string());   // Структуры
        default_palette.insert("3".to_string(), "\x1b[3;32m".to_string());   // Литералы
        default_palette.insert("4".to_string(), "\x1b[0;92m".to_string());   // Конструкторы
        default_palette.insert("5".to_string(), "\x1b[0;95m".to_string());   // Трейты
        default_palette.insert("6".to_string(), "\x1b[0;96m".to_string());   // Операторы
        default_palette.insert("7".to_string(), "\x1b[1;34m".to_string());   // Макросы
        default_palette.insert("8".to_string(), "\x1b[0;93m".to_string());   // Вызовы методов
        default_palette.insert("9".to_string(), "\x1b[4;32m".to_string());   // Импорты
        default_palette.insert("10".to_string(), "\x1b[0;36m".to_string());
        default_palette.insert("13".to_string(), "\x1b[3m".to_string());     // Свойства
        default_palette.insert("14".to_string(), "\x1b[0;33m".to_string());    // Строки
        default_palette.insert("15".to_string(), "\x1b[0;32m".to_string());
        default_palette.insert("default".to_string(), "\x1b[0m".to_string());

        let mut python_palette = default_palette.clone();
        python_palette.insert("6".to_string(), "\x1b[1;34m".to_string());

        let mut color_palettes = HashMap::new();
        color_palettes.insert("py".to_string(), python_palette);

        Self { file_types, lsp_servers, color_palettes, default_palette }
    }
}

impl Configuration {
    fn get_config_path() -> PathBuf {
        let mut path = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
        path.push(".boroda.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::get_config_path();

        if !path.exists() {
            let default_config = Self::default();
            if let Ok(json_str) = serde_json::to_string_pretty(&default_config) {
                if let Ok(mut file) = File::create(&path) {
                    let _ = file.write_all(json_str.as_bytes());
                }
            }
            return default_config;
        }

        if let Ok(mut file) = File::open(&path) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(config) = serde_json::from_str(&contents) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::OnceLock<Configuration> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(Self::load)
    }

    /// НОВОЕ: Преобразует текстовую карту цветов из JSON в плоский быстрый вектор.
    /// Размер вектора по умолчанию берем 32 (хватит для большинства стандартных токенов LSP).
    pub fn compile_palette_to_vector(&self, ext: &str) -> Vec<String> {
        let default_reset = self.default_palette.get("default").cloned().unwrap_or_else(|| "\x1b[0m".to_string());
        let mut vec_palette = vec![default_reset.clone(); 32];

        // Определяем, какую сырую палитру взять
        let raw_palette = self.color_palettes.get(ext).unwrap_or(&self.default_palette);

        // Заполняем вектор значениями
        for (key, color) in raw_palette {
            if let Ok(index) = key.parse::<usize>() {
                if index >= vec_palette.len() {
                    // Если у пользователя кастомный ID токена больше 31, динамически расширяем вектор
                    vec_palette.resize(index + 1, default_reset.clone());
                }
                vec_palette[index] = color.clone();
            }
        }

        vec_palette
    }
}


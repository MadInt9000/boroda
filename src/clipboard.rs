use std::sync::Mutex;
use arboard::Clipboard as SystemClipboard;

pub struct ClipboardManager {
    // Используем Mutex для потокобезопасного глобального доступа
    system_clipboard: Option<Mutex<SystemClipboard>>,
    fallback_clipboard: Mutex<String>,
}

impl ClipboardManager {
    fn new() -> Self {
        let sys_clip = SystemClipboard::new()
            .map(Mutex::new)
            .ok(); // Если ОС не поддерживает или нет оконного менеджера, будет None

        Self {
            system_clipboard: sys_clip,
            fallback_clipboard: Mutex::new(String::new()),
        }
    }

    /// Потокобезопасный доступ к глобальному менеджеру буфера обмена
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::OnceLock<ClipboardManager> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    /// Записать текст в буфер обмена
    pub fn set_text(&self, text: String) {
        if let Some(ref sys_mutex) = self.system_clipboard {
            if let Ok(mut sys) = sys_mutex.lock() {
                if sys.set_text(text.clone()).is_ok() {
                    return;
                }
            }
        }
        // Fallback, если системный буфер не отработал
        if let Ok(mut fallback) = self.fallback_clipboard.lock() {
            *fallback = text;
        }
    }

    /// Получить текст из буфера обмена
    pub fn get_text(&self) -> String {
        if let Some(ref sys_mutex) = self.system_clipboard {
            if let Ok(mut sys) = sys_mutex.lock() {
                if let Ok(text) = sys.get_text() {
                    return text;
                }
            }
        }
        // Fallback
        if let Ok(fallback) = self.fallback_clipboard.lock() {
            return fallback.clone();
        }
        String::new()
    }
}

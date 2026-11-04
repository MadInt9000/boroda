// Общие типы для отрисовки и интерактивного редактирования

#[derive(Clone, Copy, Debug)]
pub struct RenderToken {
    pub start_col: usize,
    pub len: usize,
    pub token_type: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub start_x: usize,
    pub start_y: usize,
}

pub struct LspDiagnostic {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    /// 1 — ошибка, 2 — предупреждение, остальное — прочее.
    pub severity: u8,
    pub message: String,
}

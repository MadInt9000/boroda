// Единая структура токена для подсветки синтаксиса
#[derive(Clone, Copy, Debug)]
pub struct RenderToken {
    pub start_col: usize,
    pub len: usize,
    pub token_type: u32,
}

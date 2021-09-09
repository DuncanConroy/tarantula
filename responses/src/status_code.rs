use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StatusCode {
    pub code: u16,
    pub label: String,
}

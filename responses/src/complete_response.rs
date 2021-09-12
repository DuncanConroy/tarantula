use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct CompleteResponse {
    pub uuid: Uuid,
}
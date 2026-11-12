use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct UpdateMatchReq {
    pub winner_id: Option<Uuid>,
}

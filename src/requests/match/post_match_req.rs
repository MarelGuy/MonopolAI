use serde::Deserialize;
use uuid::Uuid;

use crate::enums::Piece;

#[derive(Debug, Deserialize)]
pub struct PostMatchReq {
    pub players: Vec<(Uuid, Piece)>,
}

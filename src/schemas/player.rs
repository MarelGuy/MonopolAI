use uuid::Uuid;

use crate::{enums::Piece, schemas::Position};

pub struct Player {
    id: Uuid,
    piece: Piece,
    money: u16,
    name: String,
    old_position: Position,
    current_position: Position,
}

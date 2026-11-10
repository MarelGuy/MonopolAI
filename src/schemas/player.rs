use uuid::Uuid;

use crate::{
    enums::Piece,
    schemas::{Position, Turn},
};

#[derive(sqlx::FromRow)]
pub struct Player {
    id: Uuid,
    piece: Piece,
    money: u16,
    name: String,
    turns: Vec<Turn>,
    old_position: Position,
    current_position: Position,
}

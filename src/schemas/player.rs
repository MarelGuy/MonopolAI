use uuid::Uuid;

use crate::{
    enums::Piece,
    schemas::{Space, Turn},
};

#[derive(sqlx::FromRow)]
pub struct Player<'a> {
    id: Uuid,
    piece: Piece,
    money: u16,
    name: String,
    turns: Vec<Turn>,
    old_position: &'a Space,
    current_position: &'a Space,
}

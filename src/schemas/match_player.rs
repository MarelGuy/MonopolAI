use crate::{
    enums::Piece,
    schemas::{Player, Space},
};

pub struct MatchPlayer {
    pub player: Player,
    piece: Piece,
    money: u16,
    old_position: Option<Space>,
    current_position: Space,
}

impl MatchPlayer {
    pub fn new(player: (Player, Piece)) -> Self {
        Self {
            player: player.0,
            piece: player.1,
            money: 0,
            old_position: None,
            current_position: Space::default(),
        }
    }
}

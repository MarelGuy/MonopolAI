use serde::Serialize;

use crate::{
    enums::Piece,
    schemas::{Player, game::Space},
};

#[derive(Clone, Serialize)]
pub struct MatchPlayer {
    pub player: Player,
    pub piece: Piece,
    pub money: u16,
    pub old_position: Option<Space>,
    pub current_position: Space,
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

    pub(crate) const fn from_parts(
        player: Player,
        piece: Piece,
        money: u16,
        old_position: Option<Space>,
        current_position: Space,
    ) -> Self {
        Self {
            player,
            piece,
            money,
            old_position,
            current_position,
        }
    }
}

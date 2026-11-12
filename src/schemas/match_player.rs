use serde::Serialize;

use crate::{
    enums::Piece,
    schemas::{Player, Space},
};

#[derive(Clone, Serialize)]
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

    pub(crate) const fn piece(&self) -> &Piece {
        &self.piece
    }

    pub(crate) const fn money(&self) -> u16 {
        self.money
    }

    pub(crate) fn old_position(&self) -> Option<&Space> {
        self.old_position.as_ref()
    }

    pub(crate) const fn current_position(&self) -> &Space {
        &self.current_position
    }
}

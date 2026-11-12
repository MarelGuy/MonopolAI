use serde::Serialize;
use uuid::Uuid;

use crate::{enums::Actions, schemas::MatchPlayer};

#[derive(sqlx::FromRow, Serialize)]
pub struct Turn {
    id: Uuid,
    dice_result: u8,
    action: Actions,
    player: MatchPlayer,
}

impl Turn {
    pub(crate) const fn from_parts(
        id: Uuid,
        dice_result: u8,
        action: Actions,
        player: MatchPlayer,
    ) -> Self {
        Self {
            id,
            dice_result,
            action,
            player,
        }
    }
}

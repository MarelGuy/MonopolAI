use crate::enums::Actions;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct Turn {
    id: Uuid,
    dice_result: u8,
    action: Actions,
}

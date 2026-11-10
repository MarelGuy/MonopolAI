use crate::enums::SpaceType;

#[derive(sqlx::FromRow)]
pub struct Position {
    id: u8,
    space: SpaceType,
}

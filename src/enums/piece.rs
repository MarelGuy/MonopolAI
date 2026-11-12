use serde::{Deserialize, Serialize};

#[derive(sqlx::Type, Debug, Deserialize, Serialize, Clone)]
#[sqlx(rename_all = "PascalCase")]
pub enum Piece {
    Cat,
    Car,
    Shoe,
    Dog,
    Tubehat,
}

use serde::{Deserialize, Serialize};

#[derive(sqlx::Type, Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
#[sqlx(rename_all = "PascalCase")]
pub enum Piece {
    Cat,
    Car,
    Shoe,
    Dog,
    Tubehat,
}

use serde::Deserialize;

#[derive(sqlx::Type, Debug, Deserialize)]
#[sqlx(rename_all = "PascalCase")]
pub enum Piece {
    Cat,
    Car,
    Shoe,
    Dog,
    Tubehat,
}

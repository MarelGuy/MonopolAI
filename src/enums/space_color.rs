use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum SpaceColor {
    Brown,
    LightRed,
    DarkRed,
    Blue,
    Orange,
    Yellow,
    Green,
    DarkBlue,
}

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
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

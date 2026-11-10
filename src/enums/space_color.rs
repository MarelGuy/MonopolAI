use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
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

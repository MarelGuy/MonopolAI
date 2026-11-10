use serde::Deserialize;

use crate::enums::{SpaceColor, SpaceType};

#[derive(Deserialize, Clone, Debug)]
pub struct Space {
    color: Option<SpaceColor>,
    s_type: SpaceType,
    cost: Option<u16>,
    name: Option<String>,
}

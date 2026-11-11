use serde::{Deserialize, Serialize};

use crate::enums::{SpaceColor, SpaceType};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Space {
    color: Option<SpaceColor>,
    s_type: SpaceType,
    cost: Option<u16>,
    name: Option<String>,
}

impl Default for Space {
    fn default() -> Self {
        Self {
            color: None,
            s_type: SpaceType::Start,
            cost: None,
            name: None,
        }
    }
}

use serde::{Deserialize, Serialize};

use crate::{
    enums::{SpaceColor, SpaceType},
    schemas::game::Rent,
};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Space {
    color: Option<SpaceColor>,
    s_type: SpaceType,
    cost: Option<u16>,
    name: Option<String>,
    rent: Option<Rent>,
}

impl Default for Space {
    fn default() -> Self {
        Self {
            color: None,
            s_type: SpaceType::Start,
            cost: None,
            name: None,
            rent: None,
        }
    }
}

impl Space {
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub const fn space_type(&self) -> &SpaceType {
        &self.s_type
    }

    #[must_use]
    pub const fn cost(&self) -> Option<u16> {
        self.cost
    }

    #[must_use]
    pub const fn color(&self) -> Option<&SpaceColor> {
        self.color.as_ref()
    }
}

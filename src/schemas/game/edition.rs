use serde::Deserialize;

use crate::schemas::game::Space;

#[derive(Deserialize, Clone, Debug)]
pub struct Edition {
    pub version: String,
    pub spaces: Vec<Space>,
    pub community_chest_cards: Vec<String>,
    pub chance_cards: Vec<String>,
}

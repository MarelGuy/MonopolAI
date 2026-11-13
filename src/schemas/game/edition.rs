use serde::Deserialize;

use crate::schemas::game::{Card, Space};

#[derive(Deserialize, Clone, Debug)]
pub struct Edition {
    pub name: String,
    pub version: String,
    pub spaces: Vec<Space>,
    pub cards: Vec<Card>,
}

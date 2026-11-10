use serde::Deserialize;

use crate::schemas::Space;

#[derive(Deserialize, Clone, Debug)]
pub struct Version {
    pub version: String,
    pub spaces: Vec<Space>,
}

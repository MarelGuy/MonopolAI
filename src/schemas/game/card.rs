use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Card {
    content: String,
    is_chance: bool,
    // if true, can be kept
    is_jail: bool,
}

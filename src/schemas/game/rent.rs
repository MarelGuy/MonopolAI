use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Rent {
    base: u16,
    // houses[0] = 1 house, houses[1] = 2 houses, houses[2] = 3 houses, houses[3] = 4 houses, houses[4] = hotel
    houses: Option<[u16; 5]>,
    mortgage: u16,
    // Cash cost of one house; a hotel costs the same plus four returned houses.
    build_cost: Option<u16>,
    dice_multiplier: bool,
    train_station: bool,
}

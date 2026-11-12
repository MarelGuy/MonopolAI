use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum SpaceType {
    Start,
    CommunityChest,
    Chance,
    Property,
    TrainStation,
    IncomeTax,
    ElectricCompany,
    WaterWorks,
    SuperTax,
    JustVisiting,
    FreeParking,
    GoToJail,
}

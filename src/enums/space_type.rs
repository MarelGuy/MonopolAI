use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
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

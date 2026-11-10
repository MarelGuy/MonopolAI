use crate::enums::SpaceColor;

pub enum SpaceType {
    Start,
    CommunityChest,
    Chance,
    Property(SpaceColor, u8),
    TrainStation(u8),
    IncomeTax,
    ElectricCompany,
    WaterWorks,
    SuperTax,
}

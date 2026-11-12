#[derive(sqlx::Type, serde::Serialize, Clone, Debug)]
#[sqlx(rename_all = "PascalCase")]
pub enum Actions {
    BuyProperty,
    BuyHome,
    BuyHotel,
    SellHotel,
    SellHome,
    MortgageProperty,
    UnmortgageProperty,
    PayRent,
    PayTax,
    ChanceCard,
    CommunityChestCard,
    GoToJail,
    JailTurn,
    GetOutOfJail,
    Nothing,
}

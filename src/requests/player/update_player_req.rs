use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdatePlayerReq {
    pub name: Option<String>,
    pub wins: Option<usize>,
}

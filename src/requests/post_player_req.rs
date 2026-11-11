use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PostPlayerReq {
    pub name: String,
}

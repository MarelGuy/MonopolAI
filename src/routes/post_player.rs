use axum::{Json, extract::State};

use crate::{appstate::AppState, merror::MError, requests::player::PostPlayerReq, schemas::Player};

pub async fn post_player_handler(
    State(state): State<AppState>,
    Json(req): Json<PostPlayerReq>,
) -> Result<Json<()>, MError> {
    let player = Player::new(req.name);

    player.insert(&state.pool).await?;

    Ok(Json(()))
}

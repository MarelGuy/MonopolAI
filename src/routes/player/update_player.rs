use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MResult, not_found},
    requests::player::UpdatePlayerReq,
    schemas::Player,
};

pub async fn update_player_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePlayerReq>,
) -> MResult<Json<Player>> {
    let Some(player) = Player::update(&state, id, req.name, req.wins).await? else {
        return Err(not_found("Player not found"));
    };

    Ok(Json(player))
}

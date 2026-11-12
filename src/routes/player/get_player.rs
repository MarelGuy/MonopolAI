use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MError, not_found},
    schemas::Player,
};

pub async fn get_player_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Player>, MError> {
    let Some(player) = Player::get(&state.pool, id).await? else {
        return Err(not_found("Player not found"));
    };

    Ok(Json(player))
}

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

pub async fn delete_player_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<()>, MError> {
    let Some(()) = Player::delete(&state, id).await? else {
        return Err(not_found("Player not found"));
    };

    Ok(Json(()))
}

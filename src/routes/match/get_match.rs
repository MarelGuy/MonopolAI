use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MResult, not_found},
    schemas::Match,
};

pub async fn get_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> MResult<Json<Match>> {
    let Some(game_match) = Match::get(&state, id).await? else {
        return Err(not_found("Match not found"));
    };

    Ok(Json(game_match))
}

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MResult, not_found},
    requests::r#match::UpdateMatchReq,
    schemas::Match,
};

pub async fn update_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMatchReq>,
) -> MResult<Json<()>> {
    let Some(()) = Match::update(&state, id, req.winner_id).await? else {
        return Err(not_found("Match not found"));
    };

    Ok(Json(()))
}

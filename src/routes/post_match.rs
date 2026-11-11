use axum::{Json, extract::State};

use crate::{appstate::AppState, merror::MError, requests::PostMatchReq, schemas::Match};

pub async fn post_match_handler(
    State(state): State<AppState>,
    Json(req): Json<PostMatchReq>,
) -> Result<Json<()>, MError> {
    Match::new(req.players, &state)
        .await?
        .insert(&state.pool)
        .await?;

    Ok(Json(()))
}

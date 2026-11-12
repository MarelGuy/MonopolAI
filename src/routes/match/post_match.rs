use axum::{Json, extract::State};

use crate::{appstate::AppState, merror::MResult, requests::r#match::PostMatchReq, schemas::Match};

pub async fn post_match_handler(
    State(state): State<AppState>,
    Json(req): Json<PostMatchReq>,
) -> MResult<Json<()>> {
    Match::new(req.players, &state)
        .await?
        .insert(&state.pool)
        .await?;

    Ok(Json(()))
}

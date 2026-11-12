use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MError, not_found},
    schemas::Match,
};

pub async fn delete_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<()>, MError> {
    let Some(()) = Match::delete(&state, id).await? else {
        return Err(not_found("Match not found"));
    };

    Ok(Json(()))
}

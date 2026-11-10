use axum::{Json, extract::State};

use crate::appstate::AppState;

pub async fn health_handler(State(state): State<AppState>) -> Json<bool> {
    Json(true)
}

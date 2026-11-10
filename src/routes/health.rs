use axum::Json;

pub async fn health_handler() -> Json<bool> {
    Json(true)
}

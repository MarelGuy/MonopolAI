use axum::http::{Method, StatusCode};
use serde_json::json;

use super::common::TestApp;

#[tokio::test]
async fn health_returns_true() {
    let app = TestApp::new().await;

    let (status, body) = app.request(Method::GET, "/health", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!(true));
}

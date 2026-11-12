use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

use super::common::TestApp;

async fn post_match(app: &TestApp, players: Value) -> (StatusCode, Value) {
    app.request(Method::POST, "/match", Some(json!({ "players": players })))
        .await
}

#[tokio::test]
async fn post_match_creates_a_match() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.match_count().await, 1);
}

#[tokio::test]
async fn post_match_rejects_the_same_player_twice() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [alice, "Dog"]])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn get_match_returns_the_match() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id.to_string());
    assert_eq!(body["version"], "1.0.0");
    assert!(body["players"].is_array());
    assert!(body["turns"].is_array());
}

#[tokio::test]
async fn get_match_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_match_removes_the_match() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, _) = app
        .request(Method::DELETE, &format!("/match/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn delete_match_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(Method::DELETE, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

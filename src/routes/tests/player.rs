use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

use super::common::TestApp;

#[tokio::test]
async fn post_player_creates_a_player() {
    let app = TestApp::new().await;

    let (status, _) = app
        .request(Method::POST, "/player", Some(json!({ "name": "Alice" })))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.player_count().await, 1);
}

#[tokio::test]
async fn post_player_rejects_a_duplicate_name() {
    let app = TestApp::new().await;

    let (status, _) = app
        .request(Method::POST, "/player", Some(json!({ "name": "Alice" })))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .request(Method::POST, "/player", Some(json!({ "name": "Alice" })))
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(app.player_count().await, 1);
}

#[tokio::test]
async fn get_player_returns_the_player() {
    let app = TestApp::new().await;
    let id = app.create_player("Alice").await;

    let (status, body) = app
        .request(Method::GET, &format!("/player/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id.to_string());
    assert_eq!(body["name"], "Alice");
    assert_eq!(body["wins"], 0);
}

#[tokio::test]
async fn put_player_updates_the_player() {
    let app = TestApp::new().await;
    let id = app.create_player("Alice").await;

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/player/{id}"),
            Some(json!({ "name": "Alicia", "wins": 3 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .request(Method::GET, &format!("/player/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Alicia");
    assert_eq!(body["wins"], 3);
}

#[tokio::test]
async fn put_player_updates_only_the_given_fields() {
    let app = TestApp::new().await;
    let id = app.create_player("Alice").await;

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/player/{id}"),
            Some(json!({ "wins": 5 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .request(Method::GET, &format!("/player/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Alice");
    assert_eq!(body["wins"], 5);
}

#[tokio::test]
async fn put_player_rejects_a_duplicate_name() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    app.create_player("Bob").await;

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/player/{alice}"),
            Some(json!({ "name": "Bob" })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn put_player_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/player/{id}"),
            Some(json!({ "name": "Alice" })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_player_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(Method::GET, &format!("/player/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_player_removes_the_player() {
    let app = TestApp::new().await;
    let id = app.create_player("Alice").await;

    let (status, _) = app
        .request(Method::DELETE, &format!("/player/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .request(Method::GET, &format!("/player/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(app.player_count().await, 0);
}

#[tokio::test]
async fn delete_player_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(Method::DELETE, &format!("/player/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

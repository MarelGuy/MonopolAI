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
    assert_eq!(body["players"].as_array().unwrap().len(), 2);
    assert!(body["turns"].is_array());
}

#[tokio::test]
async fn put_match_updates_the_winner() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": alice })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["winner"]["player"]["id"], alice.to_string());
    assert_eq!(body["winner"]["player"]["name"], "Alice");
    assert_eq!(body["winner"]["piece"], "Cat");
    assert_eq!(body["winner"]["money"], 0);
    assert_eq!(body["winner"]["current_position"]["s_type"], "Start");
}

#[tokio::test]
async fn put_match_clears_the_winner() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": alice })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": null })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["winner"].is_null());
}

#[tokio::test]
async fn put_match_rejects_a_winner_outside_the_match() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;
    let carol = app.create_player("Carol").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": carol })),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_match_returns_not_found_for_an_unknown_id() {
    let app = TestApp::new().await;
    let id = Uuid::new_v4();

    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": null })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
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
async fn get_match_rejects_an_invalid_uuid() {
    let app = TestApp::new().await;

    let (status, _) = app
        .raw_request(Method::GET, "/match/not-a-uuid", None, "")
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
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

#[tokio::test]
async fn get_match_returns_full_player_state() {
    let app = TestApp::new().await;
    let alice_id = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice_id, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);

    let players = body["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);

    let alice = &players[0];
    assert_eq!(alice["player"]["id"], alice_id.to_string());
    assert_eq!(alice["player"]["name"], "Alice");
    assert_eq!(alice["player"]["wins"], 0);
    assert_eq!(alice["piece"], "Cat");
    assert_eq!(alice["money"], 0);
    assert_eq!(alice["current_position"]["s_type"], "Start");
    assert_eq!(alice["old_position"]["s_type"], "Start");
    assert!(alice["current_position"]["color"].is_null());
    assert!(alice["current_position"]["rent"].is_null());

    assert_eq!(players[1]["player"]["name"], "Bob");
    assert_eq!(players[1]["piece"], "Dog");
}

#[tokio::test]
async fn get_match_returns_persisted_turns() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let turn_id = app.insert_turn(id, alice, 7, "BuyProperty").await;

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);

    let turns = body["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["id"], turn_id.to_string());
    assert_eq!(turns[0]["dice_result"], 7);
    assert_eq!(turns[0]["action"], "BuyProperty");
    assert_eq!(turns[0]["player"]["player"]["id"], alice.to_string());
    assert_eq!(turns[0]["player"]["piece"], "Cat");
}

#[tokio::test]
async fn get_match_returns_the_stored_version() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    app.set_match_version(id, "9.9.9").await;

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "9.9.9");
}

#[tokio::test]
async fn post_match_rejects_an_unknown_player() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let unknown = Uuid::new_v4();

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [unknown, "Dog"]])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn post_match_rejects_an_empty_match() {
    let app = TestApp::new().await;

    let (status, _) = post_match(&app, json!([])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn post_match_rejects_a_single_player() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"]])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn post_match_rejects_duplicate_pieces() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Cat"]])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(app.match_count().await, 0);
}

#[tokio::test]
async fn delete_player_removes_them_from_the_match() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    assert_eq!(app.match_player_count(id).await, 2);

    let (status, _) = app
        .request(Method::DELETE, &format!("/player/{alice}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.match_player_count(id).await, 1);

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    let players = body["players"].as_array().unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0]["player"]["id"], bob.to_string());
}

#[tokio::test]
async fn delete_winner_clears_the_match_winner() {
    let app = TestApp::new().await;
    let alice = app.create_player("Alice").await;
    let bob = app.create_player("Bob").await;

    let (status, _) = post_match(&app, json!([[alice, "Cat"], [bob, "Dog"]])).await;
    assert_eq!(status, StatusCode::OK);

    let id = app.latest_match_id().await;
    let (status, _) = app
        .request(
            Method::PUT,
            &format!("/match/{id}"),
            Some(json!({ "winner_id": alice })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.match_winner_id(id).await, Some(alice));

    let (status, _) = app
        .request(Method::DELETE, &format!("/player/{alice}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(app.match_winner_id(id).await, None);
    assert_eq!(app.match_player_count(id).await, 1);

    let (status, body) = app
        .request(Method::GET, &format!("/match/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["winner"].is_null());
}

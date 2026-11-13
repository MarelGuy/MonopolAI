use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{appstate::AppState, llm::ChatProvider, routes::router, schemas::game::Edition};

pub struct TestApp {
    pub router: Router,
    pub pool: SqlitePool,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::build(None).await
    }

    pub async fn new_with_llm(llm: Arc<dyn ChatProvider>) -> Self {
        Self::build(Some(llm)).await
    }

    async fn build(llm: Option<Arc<dyn ChatProvider>>) -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let edition: Edition =
            serde_json::from_str(include_str!("../../../public/uk_version.json")).unwrap();

        Self {
            router: router(AppState {
                pool: pool.clone(),
                edition,
                llm,
            }),
            pool,
        }
    }

    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);

        if body.is_some() {
            request = request.header(CONTENT_TYPE, "application/json");
        }

        let body = body.map_or_else(Body::empty, |body| {
            Body::from(serde_json::to_vec(&body).unwrap())
        });

        let response = self
            .router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };

        (status, body)
    }

    pub async fn raw_request(
        &self,
        method: Method,
        uri: &str,
        content_type: Option<&str>,
        body: &str,
    ) -> (StatusCode, String) {
        let mut request = Request::builder().method(method).uri(uri);

        if let Some(content_type) = content_type {
            request = request.header(CONTENT_TYPE, content_type);
        }

        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    pub async fn create_player(&self, name: &str) -> Uuid {
        let (status, _) = self
            .request(Method::POST, "/player", Some(json!({ "name": name })))
            .await;
        assert_eq!(status, StatusCode::OK, "failed to create player {name}");

        self.player_id(name).await
    }

    pub async fn player_id(&self, name: &str) -> Uuid {
        sqlx::query_scalar("SELECT id FROM players WHERE name = ?1")
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn latest_match_id(&self) -> Uuid {
        sqlx::query_scalar("SELECT id FROM matches ORDER BY rowid DESC LIMIT 1")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn player_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM players")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn match_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM matches")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn match_player_count(&self, match_id: Uuid) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM match_players WHERE match_id = ?1")
            .bind(match_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn match_winner_id(&self, match_id: Uuid) -> Option<Uuid> {
        sqlx::query_scalar("SELECT winner_id FROM matches WHERE id = ?1")
            .bind(match_id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn set_match_version(&self, match_id: Uuid, version: &str) {
        sqlx::query("UPDATE matches SET version = ?2 WHERE id = ?1")
            .bind(match_id)
            .bind(version)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    pub async fn insert_turn(
        &self,
        match_id: Uuid,
        player_id: Uuid,
        dice_result: u8,
        action: &str,
    ) -> Uuid {
        let id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO turns (id, match_id, player_id, dice_result, action)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(match_id)
        .bind(player_id)
        .bind(i64::from(dice_result))
        .bind(action)
        .execute(&self.pool)
        .await
        .unwrap();

        id
    }
}

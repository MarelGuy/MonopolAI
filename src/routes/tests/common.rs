use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{appstate::AppState, routes::router, schemas::game::Edition};

pub struct TestApp {
    pub router: Router,
    pub pool: SqlitePool,
}

impl TestApp {
    pub async fn new() -> Self {
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
}

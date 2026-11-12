use std::net::SocketAddr;

use axum::{
    Router,
    http::HeaderValue,
    routing::{get, post},
};
use clap::Parser;
use merror::MResult;
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    appstate::AppState,
    merror::{MError, MResultExt},
    routes::{health_handler, r#match::post_match_handler, player::post_player_handler},
};

mod appstate;
mod cli;
mod enums;
mod merror;
mod requests;
mod routes;
mod schemas;

pub fn cors_layer(origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.to_vec()))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-data-key"),
        ])
}

#[tokio::main]
async fn main() -> MResult<()> {
    let args = cli::Args::parse();

    tracing_subscriber::fmt()
        .without_time()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&args.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("INFO")),
        )
        .init();

    let pool = SqlitePoolOptions::new()
        .connect("file:monopolai.db?mode=rwc")
        .await
        .map_err(|e| MError::internal(e.to_string()))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to apply database migrations")?;

    let file = tokio::fs::read_to_string(args.game_edition.as_str()).await?;

    let edition = serde_json::from_str(&file)?;

    let state = AppState { pool, edition };

    let app_cors_layer = if &args.allowed_origins == "*" {
        CorsLayer::new().allow_origin(AllowOrigin::any())
    } else {
        let allowed_origins: Vec<HeaderValue> = args
            .allowed_origins
            .split(',')
            .map(str::trim)
            .filter(|o| !o.is_empty())
            .filter_map(|o| o.parse::<HeaderValue>().ok())
            .collect();

        cors_layer(&allowed_origins)
    };

    let app = Router::new()
        .route("/match", post(post_match_handler))
        .route("/player", post(post_player_handler))
        .route("/health", get(health_handler))
        .with_state(state)
        .layer(app_cors_layer);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("Indirizzo API non valido")?;

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    tracing::info!(
        "Server API listening on http://{}:{}",
        &args.host,
        &args.port
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    Ok(())
}

use std::net::SocketAddr;

use axum::{Router, http::HeaderValue, routing::get};
use clap::Parser;
use merror::MResult;
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    appstate::AppState,
    merror::{MError, MResultExt},
    routes::health_handler,
    schemas::Version,
};

mod appstate;
mod cli;
mod enums;
mod merror;
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

    let version_file = match args.game_version.as_str() {
        "UK" => "./public/uk_version.json",
        _ => args.game_version.as_str(),
    };

    let file = tokio::fs::read_to_string(version_file).await?;

    let version: Version = serde_json::from_str(&file)?;

    let state = AppState { pool, version };

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
        .route("/health", get(health_handler))
        .with_state(state)
        .layer(app_cors_layer);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("Indirizzo API non valido")?;

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    tracing::info!(
        "Server API in ascolto su http://{} ({:?})",
        &args.host,
        listener.local_addr()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    Ok(())
}

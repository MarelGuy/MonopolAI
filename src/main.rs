use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::HeaderValue;
use clap::Parser;
use merror::MResult;
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    appstate::AppState,
    cors::cors_layer,
    llm::{ChatProvider, LlmClient},
    merror::{MError, MResultExt},
    routes::router,
    schemas::game::Edition,
};

mod appstate;
mod cli;
mod cors;
mod enums;
mod llm;
mod merror;
mod requests;
mod routes;
mod schemas;
mod tools;

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

    let url = format!("file:{}?mode=rwc", args.sqlite_file);

    let pool = SqlitePoolOptions::new()
        .connect(&url)
        .await
        .map_err(|e| MError::internal(e.to_string()))?;

    tracing::info!("SQLite pool connected on {}", url);

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to apply database migrations")?;

    let file = tokio::fs::read_to_string(args.game_edition.as_str()).await?;

    let edition: Edition = serde_json::from_str(&file)?;

    tracing::info!(
        "Loaded version: {} of edition: {} with {} spaces and {} cards",
        edition.version,
        edition.name,
        edition.spaces.len(),
        edition.cards.len()
    );

    if let Some(advice) = args.llm_api_type.advice() {
        tracing::warn!("{advice}");
    }

    let llm_base_url = args
        .llm_api_url
        .clone()
        .unwrap_or_else(|| args.llm_api_type.default_base_url().to_string());
    tracing::info!(
        "LLM API type: {} · model: {} · endpoint: {}",
        args.llm_api_type,
        args.llm_api_model,
        llm_base_url
    );

    let llm: Arc<dyn ChatProvider> = Arc::new(LlmClient::new(
        args.llm_api_type,
        llm_base_url,
        args.llm_api_key.clone(),
        args.llm_api_model.clone(),
    ));

    let state = AppState {
        pool,
        edition,
        llm: Some(llm),
    };

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

    let app = router(state).layer(app_cors_layer);

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

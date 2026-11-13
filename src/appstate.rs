use std::sync::Arc;

use sqlx::Sqlite;

use crate::{llm::ChatProvider, schemas::game::Edition};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::Pool<Sqlite>,
    pub edition: Edition,
    /// `None` in tests and in deployments where the LLM endpoint is disabled.
    pub llm: Option<Arc<dyn ChatProvider>>,
}

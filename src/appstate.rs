use sqlx::Sqlite;

use crate::schemas::Version;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::Pool<Sqlite>,
    pub version: Version,
}

use sqlx::Sqlite;

use crate::schemas::Edition;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::Pool<Sqlite>,
    pub edition: Edition,
}

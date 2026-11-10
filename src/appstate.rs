use sqlx::Sqlite;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::Pool<Sqlite>,
}

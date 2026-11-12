use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{appstate::AppState, merror::MResult};

#[derive(sqlx::FromRow, Serialize)]
pub struct Player {
    pub id: Uuid,
    pub name: String,
    pub wins: usize,
}

impl Player {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            wins: 0,
        }
    }

    pub async fn insert(&self, pool: &SqlitePool) -> MResult<()> {
        sqlx::query_file!(
            "src/queries/insert_player.sql",
            self.id,
            self.name,
            u32::try_from(self.wins)?
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn get(pool: &SqlitePool, id: Uuid) -> MResult<Option<Self>> {
        let row = sqlx::query_file!("src/queries/get_player.sql", id)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|row| Self {
            id: row.id,
            name: row.name,
            wins: row.wins as usize,
        }))
    }

    pub async fn delete(state: &AppState, id: Uuid) -> MResult<Option<()>> {
        let result = sqlx::query_file!("src/queries/delete_player.sql", id)
            .execute(&state.pool)
            .await?;

        Ok((result.rows_affected() > 0).then_some(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn inserts_and_loads_a_player() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let player = Player::new("Alice".to_string());
        player.insert(&pool).await.unwrap();

        let loaded = Player::get(&pool, player.id).await.unwrap().unwrap();

        assert_eq!(loaded.id, player.id);
        assert_eq!(loaded.name, "Alice");
        assert_eq!(loaded.wins, 0);
    }
}

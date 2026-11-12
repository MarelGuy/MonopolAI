use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MError, MResult},
};

#[derive(sqlx::FromRow, Serialize, Clone)]
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
        let result = sqlx::query_file!(
            "src/queries/insert_player.sql",
            self.id,
            self.name,
            u32::try_from(self.wins)?
        )
        .execute(pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                Err(MError::conflict("A player with this name already exists"))
            }
            Err(error) => Err(error.into()),
        }
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

use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    appstate::AppState,
    merror::{MError, MResult, bad_request},
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
        sqlx::query_file!(
            "src/queries/insert_player.sql",
            self.id,
            self.name,
            u32::try_from(self.wins)?
        )
        .execute(pool)
        .await
        .map_err(Self::unique_name_error)?;

        Ok(())
    }

    pub async fn update(
        state: &AppState,
        id: Uuid,
        name: Option<String>,
        wins: Option<usize>,
    ) -> MResult<Option<Self>> {
        let Some(mut player) = Self::get(&state.pool, id).await? else {
            return Ok(None);
        };

        if let Some(wins) = wins
            && u32::try_from(wins).is_err()
        {
            return Err(bad_request("Wins must fit in a 32-bit unsigned integer"));
        }

        if let Some(name) = name {
            player.name = name;
        }

        if let Some(wins) = wins {
            player.wins = wins;
        }

        sqlx::query_file!(
            "src/queries/update_player.sql",
            player.id,
            &player.name,
            u32::try_from(player.wins)?
        )
        .execute(&state.pool)
        .await
        .map_err(Self::unique_name_error)?;

        Ok(Some(player))
    }

    fn unique_name_error(error: sqlx::Error) -> MError {
        match error {
            sqlx::Error::Database(error) if error.is_unique_violation() => {
                MError::conflict("A player with this name already exists")
            }
            error => error.into(),
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

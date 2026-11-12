use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    appstate::AppState,
    enums::{Actions, Piece},
    merror::{MError, MResult, bad_request},
    schemas::{Edition, MatchPlayer, Player, Space, Turn},
};

#[derive(Serialize)]
pub struct Match {
    id: Uuid,
    version: String,
    players: Vec<MatchPlayer>,
    turns: Vec<Turn>,
    #[serde(skip)]
    edition: Edition,
    winner: Option<MatchPlayer>,
}

impl Match {
    pub async fn new(players: Vec<(Uuid, Piece)>, state: &AppState) -> MResult<Self> {
        let mut match_players: Vec<MatchPlayer> = vec![];

        for p_id in players {
            if let Some(player) = Player::get(&state.pool, p_id.0).await? {
                match_players.push(MatchPlayer::new((player, p_id.1)));
            } else {
                return Err(bad_request("Player does not exist"));
            }
        }

        Ok(Self {
            id: Uuid::new_v4(),
            version: state.edition.version.clone(),
            players: match_players,
            turns: vec![],
            edition: state.edition.clone(),
            winner: None,
        })
    }

    pub async fn get(state: &AppState, id: Uuid) -> MResult<Option<Self>> {
        let Some(row) = sqlx::query_file!("src/queries/get_match.sql", id)
            .fetch_optional(&state.pool)
            .await?
        else {
            return Ok(None);
        };

        let mut players = vec![];
        for row in sqlx::query_file!("src/queries/get_match_players.sql", id)
            .fetch_all(&state.pool)
            .await?
        {
            let player = Player {
                id: row.id,
                name: row.name,
                wins: row.wins as usize,
            };
            let old_position = Some(Self::space_at(&state.edition, row.old_position)?);
            let current_position = Self::space_at(&state.edition, row.current_position)?;

            players.push(MatchPlayer::from_parts(
                player,
                row.piece,
                u16::try_from(row.money)?,
                old_position,
                current_position,
            ));
        }

        let mut turns = vec![];
        for row in sqlx::query_file!("src/queries/get_match_turns.sql", id)
            .fetch_all(&state.pool)
            .await?
        {
            let player = players
                .iter()
                .find(|player| player.player.id == row.player_id)
                .cloned()
                .ok_or_else(|| MError::internal("Turn references an unknown match player"))?;

            turns.push(Turn::from_parts(
                row.id,
                u8::try_from(row.dice_result)?,
                row.action,
                player,
            ));
        }

        let winner = row.winner_id.and_then(|id| {
            players
                .iter()
                .find(|player| player.player.id == id)
                .cloned()
        });

        let mut edition = state.edition.clone();
        edition.version.clone_from(&row.version);

        Ok(Some(Self {
            id: row.id,
            version: row.version,
            players,
            turns,
            edition,
            winner,
        }))
    }

    pub async fn delete(state: &AppState, id: Uuid) -> MResult<Option<()>> {
        let result = sqlx::query_file!("src/queries/delete_match.sql", id)
            .execute(&state.pool)
            .await?;

        Ok((result.rows_affected() > 0).then_some(()))
    }

    fn space_at(edition: &Edition, position: u32) -> MResult<Space> {
        edition
            .spaces
            .get(position as usize)
            .cloned()
            .ok_or_else(|| MError::internal(format!("Invalid space position: {position}")))
    }

    pub async fn insert(&self, pool: &SqlitePool) -> MResult<()> {
        let winner_id = self.winner.as_ref().map(|p| p.player.id);

        sqlx::query_file!(
            "src/queries/insert_match.sql",
            self.id,
            &self.version,
            winner_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}

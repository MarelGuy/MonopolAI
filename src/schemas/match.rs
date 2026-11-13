use std::collections::HashSet;

use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    appstate::AppState,
    enums::{Actions, Piece},
    merror::{MError, MResult, bad_request},
    schemas::{MatchPlayer, Player, Turn, game::Edition, game::Space},
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
        if players.len() < 2 {
            return Err(bad_request("A match requires at least two players"));
        }

        let mut match_players: Vec<MatchPlayer> = vec![];
        let mut player_ids = HashSet::new();
        let mut pieces = HashSet::new();

        for (player_id, piece) in players {
            if !player_ids.insert(player_id) {
                return Err(bad_request("A player cannot join the same match twice"));
            }

            if !pieces.insert(piece.clone()) {
                return Err(bad_request(
                    "A piece cannot be used by more than one player",
                ));
            }

            if let Some(player) = Player::get(&state.pool, player_id).await? {
                match_players.push(MatchPlayer::new((player, piece)));
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

    pub async fn update(
        state: &AppState,
        id: Uuid,
        winner_id: Option<Uuid>,
    ) -> MResult<Option<()>> {
        let Some(game_match) = Self::get(state, id).await? else {
            return Ok(None);
        };

        if let Some(winner_id) = winner_id
            && !game_match
                .players
                .iter()
                .any(|player| player.player.id == winner_id)
        {
            return Err(bad_request("Winner is not a player in the match"));
        }

        sqlx::query_file!("src/queries/update_match.sql", id, winner_id)
            .execute(&state.pool)
            .await?;

        Ok(Some(()))
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
        let mut transaction = pool.begin().await?;

        sqlx::query_file!(
            "src/queries/insert_match.sql",
            self.id,
            &self.version,
            winner_id
        )
        .execute(&mut *transaction)
        .await?;

        for player in &self.players {
            let old_position = player
                .old_position
                .as_ref()
                .unwrap_or(&player.current_position);
            let old_position = Self::space_index(&self.edition, old_position)?;
            let current_position = Self::space_index(&self.edition, &player.current_position)?;

            sqlx::query_file!(
                "src/queries/insert_match_player.sql",
                self.id,
                player.player.id,
                player.piece,
                u32::from(player.money),
                old_position,
                current_position
            )
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(())
    }

    fn space_index(edition: &Edition, space: &Space) -> MResult<u32> {
        let index = edition
            .spaces
            .iter()
            .position(|candidate| candidate == space)
            .ok_or_else(|| MError::internal("Player position is not part of the edition"))?;

        Ok(u32::try_from(index)?)
    }
}

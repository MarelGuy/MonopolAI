use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    appstate::AppState,
    enums::Piece,
    merror::{MResult, bad_request},
    schemas::{Edition, MatchPlayer, Player, Turn},
};

#[derive(sqlx::FromRow)]
pub struct Match<'a> {
    id: Uuid,
    players: Vec<MatchPlayer>,
    turns: Vec<&'a Turn>,
    edition: &'a Edition,
    winner: Option<&'a MatchPlayer>,
}

impl<'a> Match<'a> {
    pub async fn new(players: Vec<(Uuid, Piece)>, state: &'a AppState) -> MResult<Self> {
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
            players: match_players,
            turns: vec![],
            edition: &state.edition,
            winner: None,
        })
    }

    pub async fn insert(&self, pool: &SqlitePool) -> MResult<()> {
        let winner_id = self.winner.map(|p| p.player.id);

        sqlx::query_file!(
            "src/queries/insert_match.sql",
            self.id,
            &self.edition.version,
            winner_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}

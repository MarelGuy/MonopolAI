use axum::{
    Router,
    routing::{get, post},
};

use crate::appstate::AppState;

mod health;
pub mod r#match;
pub mod player;

pub use health::health_handler;

#[cfg(test)]
mod tests;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/match", post(r#match::post_match_handler))
        .route(
            "/match/{id}",
            get(r#match::get_match_handler).delete(r#match::delete_match_handler),
        )
        .route("/player", post(player::post_player_handler))
        .route(
            "/player/{id}",
            get(player::get_player_handler)
                .put(player::update_player_handler)
                .delete(player::delete_player_handler),
        )
        .route("/health", get(health_handler))
        .with_state(state)
}

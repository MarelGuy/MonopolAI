CREATE TABLE IF NOT EXISTS players (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    wins INTEGER NOT NULL DEFAULT 0 CHECK (wins >= 0)
);

CREATE TABLE IF NOT EXISTS matches (
    id BLOB PRIMARY KEY NOT NULL,
    version TEXT NOT NULL,
    winner_id BLOB REFERENCES players (id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS match_players (
    match_id BLOB NOT NULL REFERENCES matches (id) ON DELETE CASCADE,
    player_id BLOB NOT NULL REFERENCES players (id) ON DELETE CASCADE,
    piece TEXT NOT NULL,
    money INTEGER NOT NULL DEFAULT 1500 CHECK (money >= 0),
    old_position INTEGER NOT NULL,
    current_position INTEGER NOT NULL,
    PRIMARY KEY (match_id, player_id)
);

CREATE TABLE IF NOT EXISTS turns (
    id BLOB PRIMARY KEY NOT NULL,
    match_id BLOB NOT NULL,
    player_id BLOB NOT NULL,
    dice_result INTEGER NOT NULL CHECK (dice_result BETWEEN 1 AND 12),
    action TEXT NOT NULL,
    FOREIGN KEY (match_id, player_id)
        REFERENCES match_players (match_id, player_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_matches_winner_id ON matches (winner_id);
CREATE INDEX IF NOT EXISTS idx_match_players_player_id ON match_players (player_id);
CREATE INDEX IF NOT EXISTS idx_turns_match_id ON turns (match_id);
CREATE INDEX IF NOT EXISTS idx_turns_player_id ON turns (player_id);

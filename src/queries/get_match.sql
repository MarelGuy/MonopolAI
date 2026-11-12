SELECT
    id AS "id: Uuid",
    version,
    winner_id AS "winner_id: Uuid"
FROM matches
WHERE id = ?1;

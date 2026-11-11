SELECT
    id AS "id: Uuid",
    name,
    wins AS "wins: u32"
FROM players
WHERE id = ?1;

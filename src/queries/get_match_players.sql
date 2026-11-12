SELECT
    p.id AS "id: Uuid",
    p.name,
    p.wins AS "wins: u32",
    mp.piece AS "piece: Piece",
    mp.money AS "money: u32",
    mp.old_position AS "old_position: u32",
    mp.current_position AS "current_position: u32"
FROM match_players AS mp
JOIN players AS p ON p.id = mp.player_id
WHERE mp.match_id = ?1
ORDER BY mp.rowid;

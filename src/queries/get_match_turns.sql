SELECT
    id AS "id: Uuid",
    dice_result AS "dice_result: u32",
    action AS "action: Actions",
    player_id AS "player_id: Uuid"
FROM turns
WHERE match_id = ?1
ORDER BY rowid;

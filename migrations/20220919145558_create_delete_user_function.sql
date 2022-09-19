CREATE FUNCTION delete_user(target_id UUID) RETURNS BOOLEAN
AS $body$
    BEGIN
        IF NOT (EXISTS (SELECT id FROM users WHERE id = target_id)) THEN
          RETURN FALSE;
        END IF;

        -- delete foreign keys that refer to users first to avoid referential
        -- integrity errors
        WITH deleted AS (
          DELETE FROM credentials
          WHERE user_id = target_id
          RETURNING *
        )
        INSERT INTO credentials_deleted SELECT * FROM deleted;

        WITH deleted AS (
          DELETE FROM sessions
          WHERE user_id = target_id
          RETURNING *
        )
        INSERT INTO sessions_deleted SELECT * FROM deleted;

        WITH deleted AS (
          DELETE FROM users
          WHERE id = target_id
          RETURNING *
        )
        INSERT INTO users_deleted SELECT * FROM deleted;

        RETURN TRUE; 
    END;
$body$ LANGUAGE PLpgSQL;

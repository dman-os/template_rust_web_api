-- Add migration script here

CREATE FUNCTION create_user(
  username CITEXT,
  email CITEXT,
  pass_hash TEXT
)
RETURNS users
AS $body$
    DECLARE
        le_user    users;
    BEGIN
        INSERT INTO users (
            username, email
        ) VALUES (
            username, email
        ) RETURNING * INTO le_user;
        INSERT INTO credentials (
            user_id, pass_hash
        ) VALUES ( 
            le_user.id, 
            pass_hash
        );
        return le_user;
    END;
$body$ LANGUAGE PLpgSQL;

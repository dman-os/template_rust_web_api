CREATE FUNCTION update_user(
  user_id UUID,
  new_username CITEXT,
  new_email CITEXT,
  new_pic_url TEXT,
  new_pass_hash TEXT
)
RETURNS SETOF users -- use SETOF to allow return of 0 rows
AS $body$
    DECLARE
        le_user    users;
    BEGIN
        UPDATE users 
        SET 
            username = COALESCE(new_username, username),
            email = COALESCE(new_email, email),
            pic_url = COALESCE(new_pic_url, pic_url)
        WHERE id = user_id 
        RETURNING * INTO le_user;

        IF NOT FOUND THEN
          RETURN;
        END IF;

        IF new_pass_hash != NULL THEN
            UPDATE credentials
            SET pass_hash = new_pass_hash
            WHERE user_id = user_id;
        END IF;
        RETURN NEXT le_user;
    END;
$body$ LANGUAGE PLpgSQL;


BEGIN;

DO $body$
    DECLARE
        -- use variables in order to be able to access properties using the dot operator
        le_user    users;
    BEGIN
        INSERT INTO users (
            id, username, email, pic_url
        ) VALUES (
            'add83cdf-2ab3-443f-84dd-476d7984cf75'::uuid,
            'sabrina',
            'hex.queen@teen.dj',
            'https://obj.teen.dj/d78xas'
        ) RETURNING * INTO le_user;
    END;
$body$ LANGUAGE PLpgSQL;

DO $body$
    DECLARE
        le_user    users;
    BEGIN
        INSERT INTO users (
            id, username, email, pic_url
        ) VALUES (
            'ce4fe993-04d6-462e-af1d-d734fcc9639d'::uuid,
            'archie',
            'archie1941@poetry.ybn',
            'https://pictu.res/01'
        ) RETURNING * INTO le_user;
    END;
$body$ LANGUAGE PLpgSQL;

DO $body$
    DECLARE
        le_user    users;
    BEGIN
        INSERT INTO users (
            id, username, email, pic_url
        ) VALUES (
            'd437e73f-4610-462c-ab22-f94b76bba83a'::uuid,
            'betty',
            'pInXy@melt.shake',
            null
        ) RETURNING * INTO le_user;
    END;
$body$ LANGUAGE PLpgSQL;

DO $body$
    DECLARE
        le_user    users;
    BEGIN
        INSERT INTO users (
            id, username, email, pic_url
        ) VALUES (
            '68cf4d43-62d2-4202-8c50-c79a5f4dd1cc'::uuid,
            'veronica',
            'trekkiegirl@ln.pi',
            'ipns://goatsie'
        ) RETURNING * INTO le_user;
    END;
$body$ LANGUAGE PLpgSQL;

-- you can bypass the DO section though
-- INSERT UPDATE STUFF
COMMIT;

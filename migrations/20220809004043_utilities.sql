CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS citext;

-- all major tables inherit from this table
create table __common(
    created_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP
    -- deleted_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP
);

CREATE FUNCTION maintain_updated_at()
RETURNS TRIGGER
AS $body$
    BEGIN
        NEW.updated_at := CURRENT_TIMESTAMP;
        RETURN NEW;
    END;
$body$ LANGUAGE PLpgSQL;


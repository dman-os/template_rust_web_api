CREATE TABLE __users_core (
    id            UUID             NOT NULL   DEFAULT uuid_generate_v4(),

    username      CITEXT           NOT NULL,
    email         CITEXT           NOT NULL,
    pic_url       TEXT
) INHERITS (__common);

CREATE TABLE users (
    PRIMARY KEY(id),
    CONSTRAINT unique_users_username  UNIQUE(username),
    CONSTRAINT unique_users_email  UNIQUE(email)
) INHERITS (__users_core);

CREATE TABLE users_deleted (
    deleted_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP
) INHERITS (__users_core);

CREATE TRIGGER maintain_updated_at_users
    AFTER UPDATE
    ON users
    FOR EACH ROW
    EXECUTE PROCEDURE maintain_updated_at();

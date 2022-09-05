CREATE TABLE __credentials_core (
    user_id        UUID           NOT NULL,
    pass_hash      TEXT           NOT NULL
) INHERITS (__common);

CREATE TABLE credentials (
    PRIMARY KEY(user_id),
    CONSTRAINT fk_user_id  FOREIGN KEY(user_id) REFERENCES users(id)
) INHERITS (__credentials_core);

CREATE TABLE credentials_delted (
    deleted_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP
) INHERITS (__credentials_core);

CREATE TRIGGER maintain_updated_at_credentials
    AFTER UPDATE
    ON credentials
    FOR EACH ROW
    EXECUTE PROCEDURE maintain_updated_at();

CREATE TABLE __sessions_core (
    token             TEXT            NOT NULL,
    user_id           UUID            NOT NULL,
    expires_at   TIMESTAMPTZ     NOT NULL
) INHERITS (__common);

CREATE TABLE sessions (
    PRIMARY KEY(token),
    CONSTRAINT fk_user_id  FOREIGN KEY(user_id) REFERENCES users(id)
) INHERITS (__sessions_core);

CREATE TABLE sessions_deleted (
    deleted_at  TIMESTAMPTZ NOT NULL    DEFAULT CURRENT_TIMESTAMP
) INHERITS (__sessions_core);

CREATE TRIGGER maintain_updated_at_sessions
    BEFORE UPDATE
    ON sessions
    FOR EACH ROW
    EXECUTE PROCEDURE maintain_updated_at();

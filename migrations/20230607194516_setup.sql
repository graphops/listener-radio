-- Add migration script here
CREATE TABLE IF NOT EXISTS messages
(
    id     BIGSERIAL PRIMARY KEY,
    message JSONB NOT NULL
);
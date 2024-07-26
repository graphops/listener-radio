CREATE TABLE IF NOT EXISTS messages
(
    id     BIGSERIAL PRIMARY KEY,
    message JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS indexer_aggregates
(
    id SERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    graph_account VARCHAR(255) NOT NULL,
    message_count BIGINT NOT NULL,
    subgraphs_count BIGINT NOT NULL,
    UNIQUE(graph_account, timestamp)
);

CREATE INDEX idx_message_nonce 
ON messages ((CAST(message->>'nonce' AS BIGINT)));

CREATE INDEX idx_message_graph_account 
ON messages ((message->>'graph_account'));

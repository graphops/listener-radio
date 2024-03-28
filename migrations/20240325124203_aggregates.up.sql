CREATE TABLE IF NOT EXISTS indexer_aggregates
(
    id SERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    graph_account VARCHAR(255) NOT NULL,
    message_count BIGINT NOT NULL,
    subgraphs_count BIGINT NOT NULL,
    UNIQUE(graph_account, timestamp)
);

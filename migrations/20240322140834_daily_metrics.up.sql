CREATE TABLE IF NOT EXISTS daily_metrics
(
    timestamp BIGINT PRIMARY KEY,
    data JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS message_receipts (
    message_id TEXT NOT NULL REFERENCES messages(id),
    agent_id TEXT NOT NULL,
    read_at TEXT NOT NULL,
    PRIMARY KEY (message_id, agent_id)
);

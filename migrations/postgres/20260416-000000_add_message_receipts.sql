CREATE TABLE IF NOT EXISTS message_receipts (
    message_id UUID NOT NULL REFERENCES messages(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, agent_id)
);

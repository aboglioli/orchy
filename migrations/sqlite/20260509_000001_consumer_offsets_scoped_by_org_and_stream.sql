ALTER TABLE consumer_offsets RENAME TO consumer_offsets_old;

CREATE TABLE consumer_offsets (
    organization TEXT NOT NULL,
    group_id TEXT NOT NULL,
    stream TEXT NOT NULL DEFAULT 'default',
    last_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (organization, group_id, stream)
);

INSERT INTO consumer_offsets (organization, group_id, stream, last_seq, updated_at)
SELECT '_legacy', group_id, 'default', last_seq, updated_at
FROM consumer_offsets_old;

DROP TABLE consumer_offsets_old;

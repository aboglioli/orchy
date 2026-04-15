-- FTS5 index for knowledge_entries (text search). Embeddings stay in knowledge_entries.embedding;
-- optional sqlite-vec ANN is separate (knowledge_vec) and not wired into search yet.

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_entries_fts USING fts5(
    knowledge_id UNINDEXED,
    path,
    title,
    content,
    tokenize = 'porter'
);

INSERT INTO knowledge_entries_fts(knowledge_id, path, title, content)
SELECT e.id, e.path, e.title, e.content FROM knowledge_entries e
WHERE NOT EXISTS (
    SELECT 1 FROM knowledge_entries_fts f WHERE f.knowledge_id = e.id
);

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_ai AFTER INSERT ON knowledge_entries BEGIN
    INSERT INTO knowledge_entries_fts(knowledge_id, path, title, content)
    VALUES (new.id, new.path, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_ad AFTER DELETE ON knowledge_entries BEGIN
    DELETE FROM knowledge_entries_fts WHERE knowledge_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_au AFTER UPDATE ON knowledge_entries BEGIN
    UPDATE knowledge_entries_fts
    SET path = new.path, title = new.title, content = new.content
    WHERE knowledge_id = old.id;
END;

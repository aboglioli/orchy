ALTER TABLE memory DROP CONSTRAINT IF EXISTS memory_pkey;
ALTER TABLE memory ADD PRIMARY KEY (project, namespace, key);

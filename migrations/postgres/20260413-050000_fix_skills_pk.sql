ALTER TABLE skills DROP CONSTRAINT IF EXISTS skills_pkey;
ALTER TABLE skills ADD PRIMARY KEY (project, namespace, name);

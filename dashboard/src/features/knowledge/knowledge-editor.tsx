import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { knowledgeService } from '../../services/knowledge/knowledge.service';

type KnowledgeEditorProps = {
  org: string;
  project: string;
};

export function KnowledgeEditor({ org, project }: KnowledgeEditorProps) {
  const [path, setPath] = useState('handoff');
  const [content, setContent] = useState('');

  const knowledgeQuery = useQuery({
    queryKey: ['knowledge', 'editor', org, project],
    queryFn: () => knowledgeService.listKnowledge(org, project),
  });

  if (knowledgeQuery.isLoading) {
    return <section>Loading editor context...</section>;
  }

  if (knowledgeQuery.isError) {
    return <section>Failed to load editor context.</section>;
  }

  return (
    <section>
      <h2>Knowledge editor</h2>
      <p>Editor placeholder that uses the knowledge service for data loading.</p>
      <label htmlFor="knowledge-path">Path</label>
      <input
        id="knowledge-path"
        value={path}
        onChange={(event) => setPath(event.target.value)}
      />
      <label htmlFor="knowledge-content">Content</label>
      <textarea
        id="knowledge-content"
        rows={6}
        value={content}
        onChange={(event) => setContent(event.target.value)}
      />
      <button type="button" disabled>
        Save (coming soon)
      </button>
      <pre>{JSON.stringify(knowledgeQuery.data, null, 2)}</pre>
    </section>
  );
}

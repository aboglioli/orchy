import { useQuery } from '@tanstack/react-query';

import { knowledgeService } from '../../services/knowledge/knowledge.service';

type KnowledgePageProps = {
  org: string;
  project: string;
};

export function KnowledgePage({ org, project }: KnowledgePageProps) {
  const knowledgeQuery = useQuery({
    queryKey: ['knowledge', org, project],
    queryFn: () => knowledgeService.listKnowledge(org, project),
  });

  if (knowledgeQuery.isLoading) {
    return <section>Loading knowledge...</section>;
  }

  if (knowledgeQuery.isError) {
    return <section>Failed to load knowledge entries.</section>;
  }

  return (
    <section>
      <h2>Knowledge</h2>
      <p>Knowledge list placeholder for {org}/{project}</p>
      <pre>{JSON.stringify(knowledgeQuery.data, null, 2)}</pre>
    </section>
  );
}

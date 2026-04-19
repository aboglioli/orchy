import { useQuery } from '@tanstack/react-query';

import { agentService } from '../../services/agents/agent.service';

type AgentsPageProps = {
  org: string;
};

export function AgentsPage({ org }: AgentsPageProps) {
  const agentsQuery = useQuery({
    queryKey: ['agents', org],
    queryFn: () => agentService.listAgents(org),
  });

  if (agentsQuery.isLoading) {
    return <section>Loading agents...</section>;
  }

  if (agentsQuery.isError) {
    return <section>Failed to load agents.</section>;
  }

  return (
    <section>
      <h2>Agents</h2>
      <p>Agent list placeholder for {org}</p>
      <pre>{JSON.stringify(agentsQuery.data, null, 2)}</pre>
    </section>
  );
}

import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { lockService } from '../../services/locks/lock.service';

type LocksPageProps = {
  org: string;
  project: string;
};

export function LocksPage({ org, project }: LocksPageProps) {
  const [name, setName] = useState('task-board');
  const lockQuery = useQuery({
    queryKey: ['lock', org, project, name],
    queryFn: () => lockService.check(org, project, name),
  });

  if (lockQuery.isLoading) {
    return <section>Loading lock...</section>;
  }

  if (lockQuery.isError) {
    return <section>Failed to load lock.</section>;
  }

  return (
    <section>
      <h2>Locks</h2>
      <label htmlFor="lock-name">Lock name</label>
      <input
        id="lock-name"
        value={name}
        onChange={(event) => setName(event.target.value)}
      />
      <p>Lock status for {org}/{project}</p>
      <pre>{JSON.stringify(lockQuery.data, null, 2)}</pre>
    </section>
  );
}

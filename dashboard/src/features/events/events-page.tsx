import { useQuery } from '@tanstack/react-query';

import { eventService } from '../../services/events/event.service';

type EventsPageProps = {
  org: string;
  project: string;
};

export function EventsPage({ org, project }: EventsPageProps) {
  const eventsQuery = useQuery({
    queryKey: ['events', org, project],
    queryFn: () => eventService.poll(org, project),
  });

  if (eventsQuery.isLoading) {
    return <section>Loading events...</section>;
  }

  if (eventsQuery.isError) {
    return <section>Failed to load events.</section>;
  }

  return (
    <section>
      <h2>Events</h2>
      <p>Events feed for {org}/{project}</p>
      <pre>{JSON.stringify(eventsQuery.data, null, 2)}</pre>
    </section>
  );
}

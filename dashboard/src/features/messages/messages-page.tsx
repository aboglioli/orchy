import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { messageService } from '../../services/messages/message.service';

type MessagesPageProps = {
  org: string;
  project: string;
};

export function MessagesPage({ org, project }: MessagesPageProps) {
  const [messageId, setMessageId] = useState('');
  const threadQuery = useQuery({
    queryKey: ['messages', 'thread', org, project, messageId],
    queryFn: () => messageService.getThread(org, project, messageId, 50),
    enabled: messageId.trim().length > 0,
  });

  return (
    <section>
      <h2>Messages</h2>
      <label htmlFor="message-id">Message id</label>
      <input
        id="message-id"
        value={messageId}
        onChange={(event) => setMessageId(event.target.value)}
      />
      <p>Thread viewer for {org}/{project}</p>
      {threadQuery.isLoading && messageId ? <p>Loading thread...</p> : null}
      {threadQuery.isError ? <p>Failed to load thread.</p> : null}
      {threadQuery.data ? <pre>{JSON.stringify(threadQuery.data, null, 2)}</pre> : null}
    </section>
  );
}

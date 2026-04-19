import { FormEvent, useState } from 'react';

import { useAuthStore } from '../../state/auth-store';

export function LoginPage() {
  const { baseUrl, apiKey, setCredentials, clearCredentials } = useAuthStore();
  const [nextBaseUrl, setNextBaseUrl] = useState(baseUrl);
  const [nextApiKey, setNextApiKey] = useState(apiKey);

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCredentials(nextBaseUrl.trim(), nextApiKey.trim());
  }

  function onClearCredentials() {
    clearCredentials();
    setNextBaseUrl('http://localhost:3100');
    setNextApiKey('');
  }

  return (
    <main>
      <h1>Dashboard login</h1>
      <form onSubmit={onSubmit}>
        <label htmlFor="base-url">Base URL</label>
        <input
          id="base-url"
          type="url"
          value={nextBaseUrl}
          onChange={(event) => setNextBaseUrl(event.target.value)}
          required
        />

        <label htmlFor="api-key">API Key</label>
        <input
          id="api-key"
          type="password"
          value={nextApiKey}
          onChange={(event) => setNextApiKey(event.target.value)}
          required
        />

        <button type="submit">Save credentials</button>
      </form>

      <button type="button" onClick={onClearCredentials}>
        Clear credentials
      </button>
    </main>
  );
}

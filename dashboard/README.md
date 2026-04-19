# Orchy Dashboard

React admin app for Orchy REST resources.

## Run

```bash
npm install
npm run dev
```

## Checks

```bash
npm test
npm run build
```

## Architecture rules

- `features` import from `services` only.
- `services` import from `infrastructure` only.
- Only `infrastructure` imports `ky`.

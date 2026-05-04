# orchy-store-conformance

Cross-backend conformance test suite. One canonical scenario set, executed against every storage backend (memory, sqlite, postgres). Catches divergence — the kind that historically bit us when SQLite tag-LIKE matched substrings, PG list omitted org filter, and memory iterated all knowledge with no index.

## What it tests

Each scenario asserts an invariant that MUST hold identically across all backends. Lives in `src/scenarios.rs`. Current set:

- `task_save_then_find_returns_same` — round-trip identity.
- `task_filter_tag_does_not_match_substring` — exact-match filtering (regression for the `auth` ⊂ `authorization` SQLite bug).
- `knowledge_optimistic_concurrency` — stale-version save returns `VersionMismatch`.
- `message_claim_visibility` — claimed logical-target message is hidden from siblings, visible to claimant.
- `edge_alias_blocks_normalizes_to_depends_on` — alias parser collapses `"blocks"` to `RelationType::DependsOn`.

## How to run

```sh
# Default: memory + sqlite
cargo test -p orchy-store-conformance

# With Postgres (requires DATABASE_URL pointing at a live PG instance with pgvector)
DATABASE_URL=postgres://orchy:orchy@localhost:5432/orchy \
  cargo test -p orchy-store-conformance --features pg-tests --test pg
```

The PG variant is gated behind the `pg-tests` Cargo feature so default workspace test runs do not require a database.

## Adding a new scenario

1. Implement `pub async fn scenario_name(bundle: &Bundle)` in `src/scenarios.rs`.
2. Use only the trait methods on `Bundle` fields (`agents`, `tasks`, etc.). No backend-specific casts.
3. Register the scenario inside the `conformance_suite!` macro in `src/lib.rs`:
   ```rust
   $crate::declare_test! { $backend, scenario_name, $crate::scenarios::scenario_name }
   ```
4. The macro automatically materialises a `#[tokio::test]` for every backend that calls `conformance_suite!`.

## Adding a new backend

Implement the `Backend` trait, returning a `Bundle` of `Arc<dyn ...Store>`. Then add a `tests/<backend>.rs` that calls `conformance_suite!(YourBackend)`. The whole scenario set runs against the new backend with zero per-scenario boilerplate.

## Why it lives here

Each store crate already ships its own per-backend integration tests (`crates/orchy-store-{memory,sqlite,pg}/tests/integration.rs`) — those exercise backend-specific quirks. The conformance suite is orthogonal: it pins down the *contract* that every backend must honour.

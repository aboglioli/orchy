# orchy

A single CLI binary, `orchy`, that gives coding agents a shared, file-backed memory.
Everything lives in a **vault**: markdown files with YAML frontmatter plus an append-only
event log. No server, no database, no daemon, no MCP endpoint — a shell is the only
integration an agent needs.

`README.md` is for people who use orchy day to day to coordinate their agents. This file is
for working on orchy itself: how it is built, the rules the code must keep, how to run and
test it, and what is known to be missing.

## Scope

Three pillars, and nothing else:

| pillar | aggregate | stored under | defining invariant |
|---|---|---|---|
| **Knowledge** | `Document`, `Skill` | `docs/<namespace>/`, `skills/<namespace>/` | identified by `id`, never by path; relations are typed and registered; a skill's name is unique per namespace and inherited downwards |
| **Work** | `Task` | `tasks/{open,done}/` | status moves only along the state machine; a parent's status is a function of its children's |
| **Conversation** | `Message` | `messages/<thread>/` | a message informs and never tracks work; addressed by alias, role, namespace or broadcast |

Supporting them: `graph` (relations across pillars), `search` (lexical ranking across
documents and skills), `actor` (identity, roster, presence, leases) and the event log. The pillars are peers: none imports another. Where they refer
to each other they go through `EntityRef` and the relation registry.

Two things stay separate: **the tool** (this repository) and **the vault** (a directory
whose location is configuration). Never hard-code a vault path.

## Non-negotiables

- **orchy never invokes git.** The vault must stay git-compatible (`.gitignore`,
  `.gitattributes`, one file per entity), but orchy never runs, reads or depends on git.
  Committing is the user's job.
- **Frontmatter is the only source of truth.** No state is derived from a path or a filename.
  Placement is a projection of frontmatter: a task lands in `tasks/done/` *because* its
  status is terminal. A file in the wrong directory is a placement error to move, never a
  reason to rewrite frontmatter. Filenames are `<id>.md` and nothing else.
- **Plain files are the storage**, not a cache in front of one. A human editing a file in
  any editor is a supported write path, and nothing has to resynchronise afterwards.
- **Agents cannot answer prompts.** Content comes from a flag or stdin; orchy never blocks
  waiting for input.

## Architecture

Rust, edition 2024, MSRV 1.89. DDD + hexagonal. Every port is declared in the domain and
implemented by a store crate. Only the CLI wires concrete types.

```
crates/
├── orchy-core/          domain: aggregates, value objects, domain events, ports. No I/O.
│   └── src/
│       ├── id.rs              Id (ULID), IdGenerator port
│       ├── namespace.rs       Namespace (/, /backend, /backend/auth)
│       ├── entity_ref.rs      EntityKind, EntityRef (`kind:id`) — how contexts refer to each other
│       ├── clock.rs           Clock port
│       ├── event.rs           DomainEvent, EventCollector, EventLog port, RecordedEvent, EventQuery
│       ├── error.rs           DomainError, ErrorCode, exit codes
│       ├── pagination.rs      Page, PageRequest
│       ├── body.rs            Body, Section (split on markdown headings)
│       ├── title.rs · tag.rs · priority.rs
│       ├── search/            Search port, SearchQuery, Passage, Hit, BM25 `score`, `rank`, `tokenise`
│       ├── document/          Document, Frontmatter, Kind, DocumentStatus, DocumentStore
│       ├── skill/             Skill, SkillName, Summary, SkillStatus, SkillStore, `in_scope`
│       ├── task/              Task, TaskStatus, TaskStore, rollup
│       ├── message/           Message, Recipient, MessageStore, ReadWatermarks
│       ├── graph/             Edge, Relation, EdgeStore, traversal
│       └── actor/             Actor, ActorId, ActorAlias, MachineId, Role, ActorStore, Lease, LeaseStore
│
├── orchy-application/   use cases, one file each: Command in, DTO out. No rules.
│                        `brief.rs` assembles the briefing `announce` returns.
│
├── orchy-store-memory/  every port in RAM — tests
├── orchy-store-vault/   every port over the filesystem
│   └── src/
│       ├── blob.rs            BlobStore seam, FsBlobStore: atomic writes, compare-and-swap
│       ├── vault.rs           id → file index, built by scanning frontmatter
│       ├── layout.rs          where each entity kind is placed
│       ├── markdown.rs · codec.rs   frontmatter + body parsing and rendering
│       ├── documents.rs · skills.rs · tasks.rs · messages.rs · edges.rs · roster.rs
│       ├── search.rs          gathers document sections and skills into passages for `score`
│       ├── eventlog.rs        EventLog over eventuary's fs backend
│       ├── watermarks.rs      per-actor inbox read watermarks
│       ├── lock.rs            file locks
│       └── time.rs            SystemClock, UlidGenerator
│
└── orchy-cli/           the `orchy` binary
    └── src/
        ├── main.rs            command dispatch
        ├── cli.rs             clap definitions
        ├── config.rs          settings.toml, orchy.toml, vault/actor resolution
        ├── container.rs       the only place that names a concrete store
        ├── init.rs            `orchy init` scaffold
        ├── resolve.rs         id prefix / suffix / title fragment → full id
        ├── output.rs          text vs --json rendering
        ├── stdin.rs · error.rs
        └── cmd/               doc.rs, skill.rs, task.rs, msg.rs, lock.rs, brief.rs (announce, guide)
```

### Layer rules

| crate | may depend on | must not |
|---|---|---|
| `orchy-core` | stdlib, `chrono`, `serde`, `serde_json`, `thiserror`, `ulid`, `sha2`, `hex`, `async-trait`, `rust-stemmers`, `eventuary` (value types `Topic` and `Payload` only) | any store, any I/O, `tokio`, `orchy-application` |
| `orchy-application` | `orchy-core`, `async-trait`, `serde`, `chrono`, `thiserror` | any `orchy-store-*`, the CLI |
| `orchy-store-*` | `orchy-core`, their own infrastructure deps | `orchy-application` (outside tests), the CLI, each other (outside tests) |
| `orchy-cli` | everything, but concrete stores **only in `container.rs`** | domain aggregates in command handlers |

The sanctioned exception: tests may use real in-memory ports. `orchy-application` tests
import `orchy-store-memory`, and `orchy-store-vault` tests import `orchy-application` and
`orchy-store-memory`.

### Domain vs application

- **Domain answers "what is always true".** Examples: `pending` cannot jump to `completed`;
  only the holder may finish a task; a parent whose children all failed or completed takes
  a derived status (`task::rollup::resolve`, a pure domain service); a relation connects
  only the kinds it declares.
- **Application answers "what happens when someone asks for X"**: load, call the aggregate,
  persist, roll up ancestors, release the lease, return a DTO. If a use case branches on a
  domain enum to decide whether something is *allowed*, the rule has leaked; move it into
  the aggregate.
- **Stores persist and load.** They never recompute a parent or enforce a rule.

## Key patterns

### Aggregates and constructors

| pattern | purpose | events? |
|---|---|---|
| `Task::create(...)`, `Document::create(...)`, `Skill::create(...)` | first-time creation with validation | yes |
| `Task::new(RestoreTask { .. })` | reconstruction from storage, no validation | no |

`Restore*` structs have public named fields; never positional parameters. Value objects
(`Id`, `Namespace`, `ActorAlias`, `Role`, `Tag`, `Title`, `ResourceKey`, …) are validated
in `new` and implement `FromStr` / `TryFrom<String>`. Never construct one by casting.

### Events

Every mutation collects a semantic event into the aggregate's `EventCollector`. `save(&mut
entity)` writes the file, drains the collector and appends the events through the
`EventLog` port:

```
aggregate mutation → collector.collect() → store.save(&mut e) → drain() → EventLog::append()
```

The vault's log is eventuary's fs backend under `events/<machine>/`, one root per machine,
partitioned (default 10, fixed at creation, configurable in `orchy.toml` `[events]
partitions`). Topics are dotted (`task.claimed`, `document.section_replaced`,
`message.sent`, `edge.created`). `orchy events` replays them.

The workspace depends on `eventuary` through a git tag (`v0.3.0-rc.4`) with the `fs` and
`memory` features. `eventuary` and `eventuary-fs` `0.3.0-rc.4` are on crates.io, so the
comment in `Cargo.toml` saying it waits for a release is stale. Switch to
`version = "=0.3.0-rc.4"` before publishing orchy: `cargo publish` refuses git dependencies.

### Errors

```
orchy-core        DomainError { Validation, InvalidTransition, NotFound, Conflict,
                                Forbidden, UnknownType, UnknownRelation, Ambiguous }
                  ErrorCode   → exit code; orchy_core::Result<T> = Result<T, DomainError>
orchy-application ApplicationError { Domain(#[from] DomainError), Storage(String) }
                  ApplicationResult<T>
orchy-cli         CliError { Application, Config, Io, NotAVault }
```

Exit codes are part of the CLI contract, because agents branch on them:

| code | cause |
|---|---|
| 4 | `NotFound`, `NotAVault` |
| 5 | `Conflict`, `InvalidTransition`, `Forbidden` |
| 6 | `Validation`, `UnknownType`, `UnknownRelation`, `Config` |
| 7 | `Ambiguous` |
| 8 | `Storage`, `Io` |

Constructors: `DomainError::validation(..)`, `invalid_transition(from, to)`,
`not_found(resource, id)`, `conflict(..)`, `forbidden(..)`.

### Use cases

One file per use case in `orchy-application/src/`, each with a `*Command` struct and an
`execute` method. Dependencies come in through the constructor as `Arc<dyn Port>`, never as
`execute` arguments. Commands carry `String` fields; value objects are parsed inside
`execute`. Responses are DTOs from `dto.rs` (`TaskDto`, `DocumentDto`, …), never aggregates.
`Application::new(ApplicationDeps)` wires every use case.

### Vault storage

- **Index by id.** `Vault::open` scans every markdown file and maps frontmatter `id` →
  file, so files can be moved or renamed freely.
- **Layout.** `docs/<namespace>/<id>.md`, `skills/<namespace>/<name>.md`,
  `tasks/open|done/<id>.md`, `messages/<thread>/<id>.md`, `agents/<alias>@<machine>.md`. The
  roots `docs`, `skills`, `tasks`, `messages`, `agents`, `events` and `.orchy` are fixed. A
  skill is the one entity filed by name, because its name is unique per namespace.
- **Atomic writes.** Temp file, fsync, rename.
- **Preconditions.** A save with `Precondition::Unchanged` succeeds only if the file still
  digests to what this process last read (compare-and-swap under a per-file guard in
  `.orchy/write-guards/`). Two agents that load and change one entity get a conflict, not a
  lost update. Document edits additionally support `--if-match <content_hash>` across
  commands.
- **Runtime state** lives in `.orchy/` and is never committed: `presence/`, `read/`
  (watermarks), `locks/` (leases), `write-guards/`.
- **A document's or skill's own frontmatter** (fields orchy does not model) survives orchy's
  writes. `orchy skill set` writes such fields; `skill::managed_field` lists the ones it
  refuses.
- **Projected fields** (`superseded_by`, `derives`, `produced_by`, `subtasks`) are rendered
  from edges and refused by `orchy set`.

### Sharing a vault

- **Same machine.** Agents on one machine share the folder directly. Every change is
  visible on the next command, because `Vault::open` re-indexes each run.
- **Across machines.** Sharing goes through git, which orchy never runs. Each machine gets
  a `MachineId` and its own event-log root, `events/<machine>/`, because file locks cannot
  coordinate offsets across a remote. `orchy init` marks `events/** -merge` in
  `.gitattributes` so git never line-merges two logs.
- **Machine-local state.** Presence and leases never cross machines.

### Briefing

`orchy announce` saves the actor and returns `Brief`'s `BriefingDto` for the actor's
namespace:

- the skills in force;
- the unread message count;
- the tasks the actor holds;
- the next pending task;
- the latest `context` document.

`orchy guide` and a bare `orchy` print the same orientation without touching the roster.
Agents are told to run `announce` first, so this is the text every session starts from:
change it deliberately.

### Identity, presence, leases

- **Identity.** An actor is `ActorId` = `alias@machine`. `MachineId` is a ULID generated
  once and stored in `$XDG_CONFIG_HOME/orchy/settings.toml`; it separates this machine's
  event log and actors from every other's. The alias is 2–32 characters: lowercase, digits
  and `-`.
- **Roster.** `orchy announce` writes `agents/<id>.md` (roles, namespace) and refreshes
  presence. Presence is same-machine only: `agents --live` means seen in the last 300 s.
- **Leases** (`LeaseStore`) are TTL-based, same-machine, and carry a generation counter.
  Expiry is a timestamp checked by the reader; nothing reaps them. `orchy lock` exposes them
  directly (default TTL 300 s). Claiming a task takes the lease `task:<id>` (default 900 s)
  so racing claimers cannot both win.

## Domain rules

### Tasks

```
pending ─▶ claimed ─▶ in_progress
   ▲          │            │
   └─release──┴────────────┤
                           ▼
        completed · failed · cancelled · superseded   (terminal, absorbing)

pending | claimed | in_progress ─block─▶ blocked ─unblock─▶ pending
pending | blocked | claimed | in_progress ─▶ cancelled | superseded
```

- **Claiming.** Only `pending` is claimable, and claiming is not a self-transition.
- **Holder only.** Completing, failing, cancelling a claimed task and releasing it are
  restricted to the holder.
- **No reclaim.** A claimed task is never taken over; it returns to `pending` only through
  `release`.
- **Rollup** (`task::rollup::resolve`) runs when a child reaches a terminal status. While
  any child is open it yields nothing. Otherwise the parent takes:
  - `failed` if any child failed;
  - `completed` if any child completed;
  - `superseded` if every child was superseded;
  - `cancelled` in every remaining case.

  `cancelled` and `superseded` are neutral: they carry no verdict. Rollup recurses up to
  `MAX_DEPTH` (64) and stops on cycles.
- **Split vs replace.** `split` keeps the original as an umbrella that waits for its new
  children. `replace` supersedes the original; the new tasks inherit its parent and get
  `supersedes` edges to it.
- **Next.** `task next` considers `pending` tasks with an empty `depends_on`, ranks them by
  priority (`urgent > high > normal > low`), then by age, then by id, and walks down the
  ranking on contention. Dependencies are not cleared automatically when the work they point
  at finishes.

### Documents

- **Kinds.** Fifteen, in `Kind::ALL`: `note`, `decision`, `discovery`, `pattern`,
  `document`, `config`, `reference`, `plan`, `log`, `skill`, `overview`, `summary`,
  `report`, `context`, `candidate`.
- **Statuses.** Canon kinds use `draft | active | superseded | archived`. `candidate` uses
  `proposed | promoted | rejected`, and the two sets never overlap. Status changes are
  semantic transitions (`archive`, `unarchive`, `supersede`, `promote`), never `orchy set`.
- **Sections.** A body is split into sections by markdown headings (any level).
- **The `skill` kind.** It still exists as a document kind, and using it breaks the vault
  (see Known gaps). Binding conventions are `Skill` entities, which are what briefings carry.
- **Placement.** A hand-written document outside `docs/` is moved to
  `docs/<namespace>/<id>.md` the next time orchy saves it. Markdown files without an `id` are
  ignored.

### Skills

- **Name.** `SkillName`: 2–48 characters, lowercase, digits and `-`, unique per namespace.
  `Summary` is the single line a briefing shows.
- **Inheritance.** `skill::in_scope(skills, namespace)` resolves the active skills in force
  at a namespace: the namespace's own plus every ancestor's, with the nearest one winning
  when two share a name.
- **Statuses.** `active | retired`. Retired skills leave briefings, listings and search
  unless asked for.
- **Managed fields.** `id`, `type`, `name`, `summary`, `namespace`, `status`, `tags`,
  `created` and `updated` change only through their commands; any other field is the team's.

### Search

- **Stores gather, the domain scores.** Stores turn entities into `Passage`s (one per
  document section, one per skill) after applying the query's filters, and call
  `search::score`. They never rank.
- **Terms.** `tokenise` splits on non-alphanumerics, lowercases, and applies the English
  Snowball stemmer.
- **`score`** is BM25:
  - `k1 = 1.2`, `b = 0.75`;
  - title terms count 3× (a skill's title is its name plus summary);
  - × the fraction of query terms matched;
  - × 1.5 when the passage holds the exact phrase.

  A passage matching no term is dropped; an empty query returns everything with zero
  relevance.
- **`rank`** orders by relevance × recency (90-day decay) × namespace proximity to the
  anchor, then by recency, then by id, so ties are deterministic.

### Relations

- **Registry.** Sixteen relations in `Relation::ALL`. Endpoints are `EntityKind`s:
  `document`, `skill`, `task`, `message`, `actor`; the first four count as content. Each declares its endpoints
  (`accepts`), inverse, symmetry and arity as an exhaustive match, so adding a variant
  without deciding them does not compile.
- **Managed relations.** `parent`, `depends_on`, `supersedes` and `spawned_by` have side
  effects and are refused by `orchy link`; `managed_by()` names the command that sets each.

### Messages

- **Recipients.** `@alias`, `@alias@machine`, `role:<r>`, `ns:<path>`, `broadcast`. They are
  stored as written and resolved against the roster at read time (`Recipient::delivers_to`).
- **Statuses.** Threads are `open | resolved`.
- **Inbox.** Messages past the actor's read watermark.
- **Promote.** `msg promote` creates a task with a `spawned_by` edge and resolves the thread.

## CLI contract

Agents branch on this behaviour, so treat it as API.

- **Resolution.** `Config::resolve` (`config.rs`):
  - vault: `--vault` → `ORCHY_VAULT` → `settings.vault` → `$XDG_DATA_HOME/orchy`;
  - actor: `--actor` → `ORCHY_ACTOR` → `settings.actor` → `human`.

  A bare alias gets `@<machine>` appended.
- **Files.** `$XDG_CONFIG_HOME/orchy/settings.toml` is per machine (`machine`, `vault`,
  `actor`); `machine` is generated on first run and must never change. `<vault>/orchy.toml`
  marks a vault, and only its `[events] partitions` key is read.
- **No vault.** Only `init`, `status`, `completions` and a bare `orchy` run without one;
  everything else exits 4 and names `orchy init`.
- **Resolving ids.** Tasks and documents accept a full ULID, an id prefix, an id suffix or a
  title fragment (`resolve.rs`); more than one match is `Ambiguous` (exit 7), never a guess.
  Skills resolve by id, or by name: first in scope at `--namespace`, then anywhere if the
  name is unique. Messages and `link`/`graph` refs (`kind:id`) take full ids only.
- **Input.** Content comes from a flag or stdin, never a prompt (`stdin.rs`).
- **Output.** Every command supports `--json`. Colour is used only on a TTY, never with
  `--no-color` or `NO_COLOR`.
- **Errors.** Exit codes follow the table under Errors. clap's own usage errors exit 2.

## Known gaps

Verified against the code on 2026-09-28. Fix them or remove them from this list; do not let
it drift. The first three corrupt data or break the vault; fix them first.

- **A document of kind `skill` breaks the vault.** `orchy new skill …` or
  `orchy promote <candidate> --as skill` writes `docs/<id>.md` with `type: skill`. The
  vault's skill loading then reads it as a `Skill` entity, finds no `name`, and fails. From
  then on `announce`, `recall` and every `skill` command exit 6 until the file is fixed by
  hand. Either drop `Kind::Skill` or make `promote --as skill` create a real `Skill`.
- **`supersede` stores the edge backwards.** `orchy supersede <old> --by <new>` writes
  `supersedes: [document:<new>]` on the old document (`supersede_document.rs`), while
  `task replace` stores replacement → original. The relation reads "from supersedes to", so
  documents are the ones that are wrong.
- **`recall` returns superseded and archived documents.** The CLI never passes a status
  filter, and `RecallCommand.status` is not exposed as a flag.

- **`orchy new` ignores stdin.** Its `--body` help says it reads stdin when omitted, but
  `cmd::doc::new` passes `None` through and creates an empty body. `edit` does read stdin.
- **Rollup leaves the lease behind.** When a parent reaches a terminal status through rollup,
  its `task:<id>` lease is not released (`RollupAncestors`); `orchy lock list` still shows
  it until it expires.
- **Dependencies are never cleared.** `NextTask` skips any task whose `depends_on` is
  non-empty, even when every dependency is completed, and nothing removes them. A dependent
  task stays invisible to `task next` until someone runs `task dep --remove`.
- **Documents cannot be retitled, retyped, moved or retagged from the CLI.**
  `UpdateDocument` supports title, kind, namespace and tags, but the CLI only uses it for
  `archive`/`unarchive`. `orchy set` refuses those fields and points at commands that do
  not exist (`orchy retitle`, `orchy retype`, `orchy ns move`, `orchy tag` —
  `document::semantic_command_for`).
- **Short message ids are not resolved.** `msg inbox` prints short ids, but `msg read`,
  `thread`, `resolve` and `promote` take only full ULIDs.
- **`orchy guide` needs a vault**, although it only prints static text and its help says it
  works without joining. It exits 4 outside a vault; a bare `orchy` does not.
- **camelCase is one search term.** `tokenise` splits on non-alphanumerics only, so
  `UserRepository` never matches `repository`. There is no prefix or substring fallback
  either.
- **Vault scaffolding writes dead config.**
  - `orchy init` writes `[vault] name`, `[recall] default_limit` and
    `[audit] stale_after_days` into `orchy.toml`, and none of them is read.
  - `.gitattributes` gets `journal/** merge=union` for a `journal/` that does not exist.
- **Unused dependencies.** `orchy-cli` declares `clap_mangen` and `orchy-store-memory` and
  uses neither. The `orchy-store-memory` description mentions "ephemeral vaults", which
  nothing wires up.
- **Server-era files.** These were left behind by the move to a single binary:
  - `Dockerfile` builds `orchy-server` and copies `migrations/`, neither of which exists,
    so `.github/workflows/container.yml` fails on the next release;
  - `.dockerignore`, `config.toml`, `config.default.toml`, `examples/`, `.orchy.toml`
    (with a committed API key) and `dashboard/`, a React client for the removed REST API.
    A dashboard is an explicit non-goal.

  `.gitignore` still lists `*.db`, `keys/`, `.mcp.json` and the dashboard build paths.
- **CI is Linux only.** File-lock semantics differ on macOS, where a wrong assumption is a
  silent double claim rather than an error.
- **Spec references.** Two code comments cite decisions from `docs/spec.md` (`D33` in
  `eventlog.rs`, `D42` in `layout.rs`). That file is untracked, so the references dangle.
- **Dead source files.** These are never declared as modules, so they don't compile into
  anything:
  - `orchy-core/src/message/events.rs`;
  - `orchy-core/src/graph/events.rs`, `neighborhood.rs`, `relation_options.rs` and
    `rules.rs`.

## Code style

- No comments unless they explain something non-obvious. No TODOs, no docstrings on every
  function.
- No helper or utils files, and no generic module names (`utils`, `types`, `helpers`,
  `common`, `shared`). A type belongs at the `orchy-core` crate root only if two or more
  contexts name it in a public signature.
- Traits first in each file, then types, constructors, methods, getters.
- Return early; no `else` after `return`.
- Test names read as sentences:
  `fn a_blocked_task_must_be_unblocked_before_it_can_be_claimed()`.
- Workspace lints: `unsafe_code = "deny"`, `unreachable_pub = "warn"`, clippy `all`. CI runs
  clippy with `-D warnings`.

### Import style

Import types and use the short name everywhere; qualify only where the module adds meaning.

- **Types.** Never write `std::sync::Arc`, `chrono::DateTime` or `serde_json::Value` inline
  in signatures, fields or bindings when an import works.
- **Stdlib.** `Arc::new()`, `HashMap::new()`, `fmt::Display`, `io::Error`.
- **External functions.** Import and use short (`Utc::now()`, `Duration::seconds()`), unless
  the name is too generic to read alone.
- **Internal paths.** Import them; do not write `crate::a::b::C` in type positions.
- **Shadowed stdlib names.** When the domain `Result` shadows `std::result::Result`, import
  the latter as `StdResult`.

## Git and commits

- Conventional Commits: `type(scope): description`, lowercase, one line by default.
  Scopes are crate or context names (`core/task`, `store-vault`, `cli`).
- Branches are prefixed with the change type (`feat/…`, `fix/…`, `docs/…`). Feature work
  happens in a worktree under `.worktrees/<branch-with-slashes-as-dashes>`.
- Never push. Never stage without being asked. No `Co-Authored-By` or tool attribution.
- Do not change commit-signing settings, and never bypass signing to get a commit through.

## Documentation policy

- `docs/` is for durable, human-facing documentation: architecture notes, ADRs, operator
  guides, migration notes.
- Do not write to or commit into `docs/` unless a human explicitly asks.
- Never commit agent-only artifacts (plans, scratch analysis, investigation dumps, session
  notes). Rewrite useful insight into a concise human-facing document first.
- Keep `README.md` in step with the command surface. When a command, flag, default or exit
  code changes, update it in the same change.
- `README.md` is written for people using orchy, not for contributors: what it does, how to
  set it up, how to use each command. Keep internals (storage mechanics, scoring formulas,
  crate layout, build instructions, known gaps) here instead.

## Running

```bash
just              # list recipes
just build        # cargo build --workspace
just test         # cargo test --workspace --no-fail-fast
just lint         # cargo clippy --workspace --all-targets -- -D warnings
just fmt          # cargo fmt --all
just check        # fmt + lint + test
just t <pattern>  # matching tests, with output
just orchy <args> # cargo run -p orchy-cli -- <args>
```

Install your working copy with `cargo install --path crates/orchy-cli`.

No containers or services are needed. Vault tests run in temporary directories. The
`orchy-cli` integration tests (`tests/cli.rs`, `tests/concurrent_agents.rs`) drive the built
binary, including many agents racing for the same work.

To try the CLI without touching your real vault or settings:

```bash
export XDG_CONFIG_HOME=$(mktemp -d) ORCHY_VAULT=$(mktemp -d)/vault
just orchy init && just orchy --actor coder-1 announce --roles developer
```

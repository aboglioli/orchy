# Orchy

A shared, file-backed memory for coding agents.

orchy is a single binary, `orchy`, that gives several agents — Claude Code, Codex, Gemini
CLI, OpenCode, pi, or a shell script — one place to keep what they know, what they are
working on, and what they need to tell each other. Everything lives in a **vault**: a
directory of ordinary markdown files with YAML frontmatter, plus an append-only event log.
No server, no database, no daemon. A shell is the only integration an agent needs.

Humans can ignore the CLI entirely: open the vault in any editor, read it, fix it, commit it.

## Three pillars

| | what it is | commands |
|---|---|---|
| **Knowledge** | typed markdown documents, skills (the conventions a team expects every agent to follow), and a registered relation graph over them | `new` `read` `edit` `set` `recall` `link` `graph` `supersede` `promote` `archive` `skill …` |
| **Work** | a task board with a real state machine, where a goal can be split and its status derived from its subtasks | `task …` |
| **Conversation** | a message board agents post to, addressed by alias, role, namespace or broadcast | `msg …` |

Around them: a roster of agents that brief themselves on joining (`announce`, `agents`),
advisory locks (`lock …`) and the event log (`events`).

## Install

Requires Rust 1.89 or newer.

```bash
cargo install --git https://github.com/aboglioli/orchy orchy-cli
```

Or from a checkout:

```bash
cargo install --path crates/orchy-cli
```

Shell completions:

```bash
orchy completions fish > ~/.config/fish/completions/orchy.fish
orchy completions bash > ~/.local/share/bash-completion/completions/orchy
orchy completions zsh  > "${fpath[1]}/_orchy"
```

## Quick start

```bash
orchy init                                   # scaffold the vault (default: $XDG_DATA_HOME/orchy)
export ORCHY_ACTOR=coder-1                   # who is acting

orchy announce --roles developer             # join the roster and get a briefing
orchy skill write commits --summary "Conventional commits, one line" \
  --body "Write type(scope): description, lowercase."
orchy task new "Rotate JWT keys" --priority high --role developer --namespace /backend
orchy task next                              # claim the highest-ranked task
orchy task start rotate                      # tasks resolve by id prefix, suffix or title fragment
orchy new decision "JWT algorithm" --namespace /backend --tag auth \
  --body $'## Context\nHS256 cannot rotate.\n\n## Decision\nUse RS256.'
orchy task done rotate --note "RS256 in place, keys under /backend"
orchy msg send role:reviewer --subject "JWT" --body "RS256 landed, please review"
orchy recall rs256
```

Every command accepts `--json` for machine-readable output. `orchy guide` explains the model
without joining the roster, and a bare `orchy` prints the same guidance with the command
list.

## Configuration

orchy keeps two kinds of configuration apart: **per machine** and **per vault**.

### Per machine — `settings.toml`

`$XDG_CONFIG_HOME/orchy/settings.toml` (default `~/.config/orchy/settings.toml`):

```toml
machine = "01M3HQK02Y7WBFTTM60JJ2JAJH"   # generated on first run; never change it
vault   = "/home/me/vault"               # optional
actor   = "coder-1"                      # optional
```

`machine` separates this machine's event log and agent identities from every other
machine's. orchy writes it the first time it runs.

Resolution, highest priority first:

| | vault | actor |
|---|---|---|
| flag | `--vault <dir>` | `--actor <alias>` |
| environment | `ORCHY_VAULT` | `ORCHY_ACTOR` |
| settings | `vault` | `actor` |
| default | `$XDG_DATA_HOME/orchy` (`~/.local/share/orchy`) | `human` |

`orchy status` prints what resolved and whether the vault is initialised.

### Per vault — `orchy.toml`

Committed with the vault. orchy currently reads one setting from it:

```toml
[events]
partitions = 10   # fixed when this machine's log is first created; changing it later is refused
```

A directory without `orchy.toml` is not a vault. Every command except `init`, `status` and
`completions` refuses to run there (exit 4).

## Identity

An agent is `alias@machine`, for example `coder-1@01M3HQK02Y7WBFTTM60JJ2JAJH`. The alias is 2–32
characters: lowercase letters, digits and `-`. Pass a bare alias and orchy appends this
machine's id; pass `alias@machine` to act as a specific instance.

`orchy announce` writes the agent to the roster (`agents/<alias>@<machine>.md`) with its
roles and namespace, refreshes its presence, and returns a **briefing** for that namespace:

- how orchy works and what to run;
- the skills in force, which the agent is expected to follow;
- how many unread messages are waiting, and the tasks it already holds;
- the highest-priority pending task in its namespace;
- the latest `context` document there (the handoff from the last session).

An agent's first command should be `orchy announce`. `orchy agents` lists the roster;
`orchy agents --live` lists only agents announced on this machine in the last five
minutes. Role and namespace messages reach only agents on the roster, so announce before
you expect mail.

## The vault

```
<vault>/
├── orchy.toml            vault configuration
├── AGENTS.md             instructions for agents working in the vault
├── CLAUDE.md -> AGENTS.md
├── index.md
├── .gitignore            keeps .orchy/ and event-log locks out of git
├── .gitattributes        marks events/** as unmergeable
│
├── docs/                 documents, placed by namespace: docs/backend/auth/<id>.md
├── skills/               skills, placed by namespace and named: skills/backend/migrations.md
├── tasks/open/<id>.md    tasks that are not finished
├── tasks/done/<id>.md    tasks in a terminal status
├── messages/<thread>/<id>.md
├── agents/<alias>@<machine>.md
├── events/<machine>/     append-only event log, one per machine
└── .orchy/               runtime state: presence, read watermarks, locks. Never committed.
```

**Frontmatter is the only source of truth.** A file's location is a projection of its
frontmatter, never the reverse: a task sits in `tasks/done/` because its `status` is
terminal, not the other way round. orchy never infers state from a path or a filename.
Every entity has a stable ULID `id`; orchy finds files by the id in their frontmatter.

A task file:

```markdown
---
id: 01M3HQK063BDRXRXY728YTHX67
type: task
title: Rotate JWT keys
status: in_progress
priority: high
namespace: /backend
roles:
  - developer
claimed_by: coder-1@01M3HQK02Y7WBFTTM60JJ2JAJH
claimed_at: "2026-09-27T15:26:26Z"
created: "2026-09-27T15:26:25Z"
updated: "2026-09-27T15:26:33Z"
---

Move to RS256
```

### Writes and concurrency

- Each entity is its own file, so agents working on different things never touch the same
  bytes.
- Writes are atomic: temp file, fsync, rename. A crash never leaves a half-written document.
- A write succeeds only if the file still holds the bytes orchy last read. When two agents
  load the same entity and both change it, the second write is refused with a conflict
  rather than silently overwriting the first.
- `orchy edit --if-match <hash>` extends that check across commands: the edit is refused
  unless the document still hashes to what you read (`content_hash` in `orchy read --json`).

### Git

orchy never runs git. The vault is designed to be a git repository you commit yourself,
under whatever signing and push policy you already use. `orchy init` writes a `.gitignore`
that keeps runtime state out, and a `.gitattributes` that stops git from line-merging the
event log.

## Namespaces

A namespace is a slash-rooted path: `/`, `/backend`, `/backend/auth`. Documents, tasks and
messages each carry one; the default is `/`. Filters on a namespace include its
descendants: `--namespace /backend` matches `/backend/auth`. A `ns:/backend` message reaches
every agent announced in `/backend` or below.

## Knowledge

### Document types

`orchy types` prints the registry.

| type | use for |
|---|---|
| `note` | general observations |
| `decision` | a choice made, with its rationale |
| `discovery` | something found or learned: gotchas, constraints, findings |
| `pattern` | a recurring approach or convention |
| `document` | long-form specs, architecture, analysis |
| `config` | configuration or setup information |
| `reference` | external references and links |
| `plan` | strategies, roadmaps, approaches |
| `log` | activity or change log entries |
| `skill` | instructions, as a document. Binding conventions belong in `orchy skill` instead |
| `overview` | project summaries |
| `summary` | compact synthesized output |
| `report` | post-task write-ups, implementation reports |
| `context` | session handoff snapshots |
| `candidate` | a proposal that has not yet been accepted into canon |

Canon types move through `draft`, `active`, `superseded` and `archived`. A `candidate` has
its own lifecycle, `proposed`, `promoted` and `rejected`, and graduates into canon with
`orchy promote <candidate> --as <type>`.

### Commands

```bash
orchy new <type> <title> [--namespace /x] [--tag t]... --body "…"
orchy read <doc> [--section <heading>]
orchy edit <doc> [--section <heading> | --replace-in <text> | --replace] [--if-match <hash>] [--content "…"]
orchy set <doc> field=value [field=value]...
orchy recall <query> [--kind k]... [--tag t]... [--namespace /x] [--anchor /x] [--limit n]
orchy supersede <old> --by <new>
orchy archive <doc>
orchy unarchive <doc>
orchy promote <candidate> --as <type> [--namespace /x]
```

- **`edit`** appends by default. `--section` replaces the body under an existing heading,
  `--replace-in` replaces a piece of text that must occur exactly once, and `--replace` replaces the whole body. Content
  comes from `--content` or stdin.
- **`set`** writes inert frontmatter fields. A value parses as JSON when it can and falls
  back to a string. `status` is refused: change it with `archive`, `unarchive`, `supersede`
  or `promote`. Fields orchy maintains itself (`superseded_by`, `derives`, `produced_by`,
  `subtasks`) are refused too.
- **`recall`** searches documents and skills together; `--entity document` or
  `--entity skill` narrows it. See [Search](#search).

Commands that take a document or a task accept a full id, an id prefix, an id suffix or a
fragment of the title. When more than one entry matches, orchy refuses rather than guesses
(exit 7).

## Skills

A skill is a convention the team expects every agent to follow: how to write commits, how
migrations work, what never to touch. Skills are their own entity, not documents: each has
a unique `name` (2–48 characters: lowercase, digits, `-`), a one-line `summary` that
agents read before deciding to open it, a body, tags and a namespace.

```bash
orchy skill write <name> --summary "…" [--body "…" | --body -] [--namespace /x] [--tag t]...
orchy skill show <name|id> [--namespace /x]
orchy skill list [--namespace /x] [--tag t]... [--everywhere] [--retired]
orchy skill find <query> [--namespace /x] [--tag t]... [--retired] [--limit n]
orchy skill set <name|id> [field=value]... [--remove field]... [--tag t]... [--untag t]...
orchy skill retire <name|id>
orchy skill restore <name|id>
```

- **Inheritance.** Skills are inherited down the namespace tree. `/backend` gets every skill
  declared at `/`, and a skill with the same name declared at `/backend` overrides it there.
  `list` and `show` resolve from `--namespace` (default `/`); `list --everywhere` shows
  every skill in the vault.
- **Writing.** `write` creates a skill, or revises the one with that name in that namespace.
  A new skill needs `--summary`. `--body -` reads the body from stdin.
- **Your own fields.** `set` adds any other frontmatter a team wants on a skill. orchy keeps
  `id`, `type`, `name`, `summary`, `namespace`, `status`, `tags`, `created` and `updated`
  to itself and refuses to set them.
- **Retiring.** `retire` takes a skill out of every briefing and listing without deleting
  it; `restore` puts it back.
- **Finding one.** `find` ranks skills against free text. `--namespace` ranks skills
  declared there first instead of filtering.

## Search

`orchy recall` (documents and skills) and `orchy skill find` (skills only) share one
lexical engine:

- **Terms, not strings.** Text is split into words, lowercased and reduced to its English
  root, so `migrate`, `migrations` and `MIGRATING` all match "migration". Query words match
  independently and in any order.
- **BM25 relevance.**
  - A word few passages contain is worth more than one they all contain.
  - Repeating a word has diminishing returns.
  - A long passage does not win on length alone.
- **Extra signals.**
  - Words in a title count three times.
  - A passage carrying every query word beats one carrying some; one carrying none is not a
    hit.
  - The exact phrase as typed earns a bonus.
- **Passages.** Each section of a document (split on markdown headings) is its own passage,
  and the document's title adds weight to each. A skill's name and summary act as its title.
- **Ranking.** Relevance is then weighted by recency (a 90-day decay) and, with
  `--anchor /x`, by how close the namespace is to the anchor. `--namespace` filters; `--anchor`
  only ranks. The default limit is 20. `--json` reports each hit's `relevance`.

It is still lexical: there are no synonyms (`k8s` does not find `kubernetes`), no typo
tolerance, and no partial-word matching (`migr` finds nothing). Words split only on
characters that are not letters or digits, so a camelCase identifier such as
`UserRepository` is one term.

## Work

### Task states

```
pending ──claim──▶ claimed ──start──▶ in_progress
   ▲                  │                    │
   └────release───────┴────────────────────┤
                                           ▼
                completed · failed · cancelled · superseded   (terminal)

pending, claimed, in_progress ──block──▶ blocked ──unblock──▶ pending
```

- Only `pending` tasks can be claimed.
- A claimed task can be finished from `claimed` or `in_progress`.
- Terminal statuses are final.
- Only the agent holding a task may finish it or release it.

### Commands

```bash
orchy task new <title> [--description …] [--priority low|normal|high|urgent] [--namespace /x]
                       [--role r]... [--tag t]... [--parent <task>] [--depends-on <task>]...
orchy task list [--status s]... [--namespace /x] [--mine] [--role r] [--parent <task>] [--tag t]... [--limit n]
orchy task get <task>
orchy task next [--role r] [--namespace /x] [--peek]
orchy task claim <task> [--ttl secs] [--start]
orchy task start <task>
orchy task release <task>
orchy task done <task> [--note …]
orchy task fail <task> <reason>
orchy task cancel <task> <reason>
orchy task block <task> [--on <task>]... [--reason …]
orchy task unblock <task>
orchy task split <task> <title>...
orchy task replace <task> <title>... [--reason …]
orchy task dep <task> [--add <task>]... [--remove <task>]...
orchy task update <task> [--title …] [--description …] [--priority …] [--namespace /x]
                         [--parent <task> | --detach] [--tag t]... [--untag t]...
```

- **`next`** picks from `pending` tasks that list no dependencies, ranked by priority and
  then age, and claims the first one it wins. With several agents racing, each gets a
  different task. `--peek` looks without claiming.
- **Claims are leases.** Claiming takes a lease on `task:<id>` for 15 minutes by default
  (`--ttl`), so two agents racing for one task cannot both win. The task stays with its
  holder until it is released or finished.
- **Split and replace** answer different questions:
  - `split` keeps the goal open as an umbrella over new subtasks. When every subtask is
    terminal the goal rolls up: `failed` if any subtask failed, otherwise `completed` if any
    completed, `superseded` if all were superseded, and `cancelled` otherwise. Rollup walks
    up through grandparents.
  - `replace` retires the original as `superseded`. The new tasks stand alone, under
    whatever goal the original sat under.
- **Dependencies** are explicit. `task next` skips a task while it lists any dependency,
  so remove it with `task dep --remove` once the work it waited on is done, or claim the task
  directly. `task block --on <task>` records the dependency and blocks in one step;
  `--reason` covers blockers that are not tasks.

## Conversation

```bash
orchy msg send <recipient>... [--subject …] [--body …] [--reply-to <msg>] [--priority …]
orchy msg inbox [--all]
orchy msg read <msg>
orchy msg thread <msg>
orchy msg sent
orchy msg resolve <msg>
orchy msg promote <msg> [--title …] [--role r]...
```

| recipient | reaches |
|---|---|
| `@alias` | every instance of that alias |
| `@alias@machine` | one instance |
| `role:<role>` | every agent with that role |
| `ns:/path` | every agent in that namespace or below |
| `broadcast` | everyone except the sender |

- Recipients resolve against the roster when mail is read, not when it is sent.
- A thread is a directory, and every message in it is its own file.
- `inbox` shows what arrived after your read watermark; `msg read` advances it, and `--all`
  ignores it.
- `msg resolve` marks a thread finished.
- `msg promote` turns a message into a task linked back with `spawned_by`, and resolves the
  thread.
- Message commands take the full message id, which `--json` output prints.

## Relations

Typed, directed edges between documents, tasks, messages and agents. `orchy types` lists
them with their inverses.

```bash
orchy link document:<id> task:<id> --rel implements
orchy unlink document:<id> task:<id> --rel implements
orchy graph task:<id> [--depth n]
```

Entity refs are `kind:id`, where `kind` is one of `document`, `skill`, `task`, `message` or
`actor`, and `id` is the full id. "Content" below means a document, skill, task or message.

| relation | inverse | connects |
|---|---|---|
| `supersedes` | `superseded_by` | like to like |
| `merged_from` | `merged_into` | like to like |
| `derived_from` | `derives` | content to content |
| `summarizes` | `summarized_by` | content to content |
| `invalidates` | `invalidated_by` | content to content |
| `confirms` | `confirmed_by` | content to content |
| `supported_by` | `supports` | content to content |
| `contradicted_by` | `contradicts` | content to content |
| `related_to` | `related_to` | anything to anything |
| `depends_on` | `blocks` | task to task |
| `parent` | `subtasks` | task to task |
| `spawned_by` | `spawns` | task to message |
| `produces` | `produced_by` | task to document or skill |
| `implements` | `implemented_by` | task to document or skill |
| `owned_by` | `owns` | anything to actor |
| `reviewed_by` | `reviewed` | anything to actor |

`parent`, `depends_on`, `supersedes` and `spawned_by` have consequences beyond the edge
itself. Set them through `task update --parent`, `task dep --add`, `supersede` or
`task replace`, and `msg promote`; `orchy link` refuses them.

## Locks

Advisory, TTL-based leases, shared by every agent on the same machine:

```bash
orchy lock acquire <resource> [--ttl secs]    # default 300; fails if someone else holds it
orchy lock renew <resource> [--ttl secs]
orchy lock release <resource>
orchy lock check <resource>
orchy lock list
orchy lock with <resource> [--ttl secs] -- <command>...   # hold it exactly as long as the command runs
```

A resource is any name: a file path, a service, `migrations`. Leases expire on their own;
nothing has to clean up after a crashed agent. Lock state lives in `.orchy/locks/` and is
never committed.

## Event log

Every change is recorded as a domain event: `document.*`, `skill.*`, `task.*`, `message.*`
and `edge.*`.
Each machine appends to its own log under `events/<machine>/`, so logs from several
machines never contend.

```bash
orchy events [--topic task.] [--key <id>] [--by <actor>] [--limit n]
```

`--topic` matches by prefix.

## Exit codes

| code | meaning |
|---|---|
| 0 | success |
| 2 | bad arguments |
| 4 | not found, or not a vault |
| 5 | conflict: held by someone else, an invalid transition, or not yours to change |
| 6 | invalid input: validation, unknown type or relation, configuration |
| 7 | ambiguous: the input matched more than one entry |
| 8 | storage or I/O failure |

## Development

```bash
just            # list recipes
just build      # cargo build --workspace
just test       # cargo test --workspace
just lint       # cargo clippy --workspace --all-targets -- -D warnings
just fmt        # cargo fmt --all
just check      # fmt, lint and test
just t <pattern>          # run matching tests with output
just orchy <args>         # run the CLI from source
```

The tests need no containers or services: the vault tests run against temporary
directories.

orchy depends on [eventuary](https://github.com/aboglioli/eventuary) for its event log,
pinned by git tag until it is published on crates.io.

## License

MIT

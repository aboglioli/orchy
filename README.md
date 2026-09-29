# Orchy

A shared memory for your coding agents.

Run several agents at once — Claude Code, Codex, Gemini CLI, OpenCode, pi, a shell script —
and orchy gives them one place to keep what they know, what they are working on, and what
they need to tell each other. They drive it through a single command, `orchy`, so any agent
that can run a shell command can join in.

Everything lives in a **vault**: a folder of ordinary markdown files. There is no server,
no database and nothing running in the background. Open the vault in your editor to see what
your agents did, fix anything by hand, and commit it to git like any other notes.

## What your agents get

| | what it is |
|---|---|
| **Knowledge** | notes, decisions, discoveries and specs they write down and search later, linked to each other and to the work that produced them |
| **Skills** | the conventions you want every agent to follow — commit style, what never to touch, how migrations work. Each agent receives them when it joins |
| **Work** | a task board: agents claim tasks, split big ones into subtasks, and report what they did. A goal finishes when its subtasks do |
| **Conversation** | a message board: agents write to one another by name, by role, by area of the project, or to everyone |

Plus a roster of who is working, locks so two agents don't edit the same thing at once, and
a full history of every change.

## Install

Requires Rust 1.89 or newer.

```bash
cargo install --git https://github.com/aboglioli/orchy orchy-cli
```

Shell completions:

```bash
orchy completions fish > ~/.config/fish/completions/orchy.fish
orchy completions bash > ~/.local/share/bash-completion/completions/orchy
orchy completions zsh  > "${fpath[1]}/_orchy"
```

## Set up a vault

```bash
orchy init                     # creates ~/.local/share/orchy
orchy status                   # shows which vault and identity orchy is using
```

Pass a path to put the vault somewhere else (`orchy init ~/vault`), then point orchy at it
with `ORCHY_VAULT=~/vault` or in the settings file (see [Configuration](#configuration)).

The vault is designed to live in git. orchy never runs git itself — you commit when you
like, with your own signing and push setup. `orchy init` writes a `.gitignore` and a
`.gitattributes` so that only the files worth sharing get committed.

## Connect your agents

Each agent needs two things: a name, and the instruction to run `orchy announce` first.

**Give each agent its own name** with `ORCHY_ACTOR`, set in the environment you launch it
from:

```bash
ORCHY_ACTOR=coder-1 claude
ORCHY_ACTOR=reviewer codex
```

A name is 2–32 characters of lowercase letters, digits and `-`. Commands you run yourself
without a name act as `human`.

**Tell it to use orchy.** Add a line like this to the instructions file of the repository
the agent works in (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`…):

```markdown
Before starting, run `orchy announce --roles developer --namespace /backend` and follow what it says.
```

`orchy announce` puts the agent on the roster and answers with a briefing:

- what orchy is and which commands to use;
- the skills in force where it is working, which it is expected to follow;
- unread messages, and any tasks it already holds;
- the most urgent task waiting in its area;
- the handoff note left by the last session there.

That briefing is all an agent needs to start working. `orchy guide` prints the same
orientation without joining, and running `orchy` with no arguments shows it along with the
command list.

## A day with orchy

**Write down how you work.** Skills are the rules every agent receives on joining:

```bash
orchy skill write commits --summary "Conventional commits, one line" \
  --body "Write type(scope): description, lowercase. No trailers."
orchy skill write migrations --namespace /backend \
  --summary "Never edit an applied migration" --body "Add a new one instead."
```

**Queue up work.** Agents pick it up with `orchy task next`:

```bash
orchy task new "Rotate JWT keys" --priority high --role developer --namespace /backend \
  --description "Move from HS256 to RS256 so keys can rotate."
orchy task new "Update the auth docs" --depends-on "Rotate JWT keys"
```

**See what is happening:**

```bash
orchy task list                     # the board
orchy task get rotate               # one task, its subtasks and links
orchy agents --live                 # who is active on this machine right now
orchy msg inbox                     # messages addressed to you
orchy recall jwt rotation           # search everything your agents wrote down
orchy events --limit 20             # the latest changes, and who made them
```

**Talk to your agents:**

```bash
orchy msg send @coder-1 --body "Keep the HS256 path until Friday."
orchy msg send role:reviewer --subject "JWT" --body "RS256 landed, please review."
orchy msg send broadcast --body "Freeze merges to main for an hour."
```

**Read and fix things by hand.** Everything is a markdown file under the vault. Edit
anything in your editor; orchy picks it up on the next command.

**Commit** the vault whenever you like.

Most commands that take a task or a document accept a fragment of its title (`rotate`)
or of its id instead of the whole thing. When more than one thing matches, orchy asks you to
be more specific rather than guessing.

## Configuration

orchy keeps two kinds of configuration apart: **per machine** and **per vault**.

### Per machine — `settings.toml`

`~/.config/orchy/settings.toml` (or `$XDG_CONFIG_HOME/orchy/settings.toml`):

```toml
machine = "01M3HQK02Y7WBFTTM60JJ2JAJH"   # generated on first run; never change it
vault   = "/home/me/vault"               # optional
actor   = "human"                        # optional
```

`machine` keeps this computer's history and agents apart from any other computer sharing the
vault. orchy writes it the first time it runs.

Which vault and which name orchy uses, highest priority first:

| | vault | name |
|---|---|---|
| flag | `--vault <dir>` | `--actor <name>` |
| environment | `ORCHY_VAULT` | `ORCHY_ACTOR` |
| settings | `vault` | `actor` |
| default | `~/.local/share/orchy` | `human` |

### Per vault — `orchy.toml`

Lives in the vault and is committed with it. A folder without `orchy.toml` is not a vault,
and orchy refuses to work there until you run `orchy init`.

```toml
[events]
partitions = 10   # set once, when this machine first writes to the vault; do not change it
```

## Several agents, several machines

An agent's full identity is `name@machine`, such as `coder-1@01M3HQK02Y7WBFTTM60JJ2JAJH`.
Running `coder-1` on two computers gives you two separate agents that share the name.
`@coder-1` reaches both; `@coder-1@<machine>` reaches one.

**On one machine**, agents see each other's changes immediately: they all read and write
the same folder.

**Across machines**, the vault syncs through git. Push from one, pull on the other, and the
agents there see the new tasks, notes and messages. Each machine keeps its own history
folder, so histories from different machines never collide. `orchy agents --live` and locks
only cover agents on the same machine.

If two agents change the same task or document at the same moment, the second one gets an
error (exit 5) instead of overwriting the first. It re-reads and tries again.

## The vault

```
<vault>/
├── orchy.toml            vault settings
├── AGENTS.md             instructions for any agent that opens the vault directly
├── CLAUDE.md -> AGENTS.md
├── index.md
│
├── docs/                 notes, decisions, specs… filed by area: docs/backend/auth/<id>.md
├── skills/               conventions, filed by area and name: skills/backend/migrations.md
├── tasks/open/           tasks still in play
├── tasks/done/           finished, failed, cancelled or replaced tasks
├── messages/<thread>/    one folder per conversation, one file per message
├── agents/               the roster, one file per agent
├── events/<machine>/     the history of every change, one folder per machine
└── .orchy/               this machine's live state: who is active, locks. Never committed.
```

Every file starts with a YAML header, and **the header is what counts**. A task is done
because its header says `status: completed`, and orchy files it under `tasks/done/` as a
result. Moving a file by hand changes nothing, and neither does renaming it: every file
carries a stable `id` and orchy finds it by that. Links between files use the id, so they
survive any reorganisation.

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

### Areas of a project

A **namespace** is a path that says which part of the project something belongs to: `/`,
`/backend`, `/backend/auth`. Tasks, documents, skills, messages and agents each carry one.

- **Writes land where the agent works.** A new note, task, message or promotion goes to the
  namespace the agent announced itself in, unless `--namespace` says otherwise.
  `ORCHY_NAMESPACE=/web` does the same for one shell. An agent that never announced writes
  to `/`. Re-running `orchy announce` without `--namespace` keeps the agent where it was.

- **Filters include everything below.** `orchy task list --namespace /backend` shows
  `/backend/auth` too.
- **Skills are inherited downwards.** An agent in `/backend/auth` follows the skills of `/`,
  `/backend` and `/backend/auth`. A skill with the same name further down replaces the one
  above it.
- **Messages to an area** (`ns:/backend`) reach every agent announced there or below.

## Knowledge

```bash
orchy new <type> <title> [--namespace /x] [--tag t]... [--body "…" | --body - | < file]
orchy read <doc> [--section <heading> [--nth n]]
orchy edit <doc> [--section <heading> [--nth n] | --replace-in <text> | --replace] [--content "…"]
orchy set <doc> field=value...
orchy recall <query> [--entity document|skill] [--kind k]... [--status s]... [--tag t]... [--namespace /x] [--anchor /x] [--limit n]
orchy retitle <doc> <title>
orchy retype <doc> <type>
orchy tag <doc> [+t | -t]...
orchy ns move <doc> <namespace>
orchy supersede <old> --by <new>
orchy archive <doc>
orchy unarchive <doc>
orchy promote <candidate> --as <type> [--namespace /x]
orchy promote <candidate> --as skill --name <name> [--summary …] [--namespace /x]
orchy types                         # every document type, status and relation
```

| type | use for |
|---|---|
| `note` | general observations |
| `decision` | a choice made, with its rationale |
| `discovery` | something found or learned: gotchas, constraints, findings |
| `pattern` | a recurring approach |
| `document` | long-form specs, architecture, analysis |
| `config` | configuration or setup information |
| `reference` | external references and links |
| `plan` | strategies, roadmaps, approaches |
| `log` | activity or change log entries |
| `overview` | project summaries |
| `summary` | compact write-ups |
| `report` | post-task write-ups |
| `context` | a handoff: what was done, what is left. The latest one reaches the next agent's briefing |
| `candidate` | a proposal not yet accepted |

For rules every agent must follow, use [`orchy skill`](#skills), not a document.

- **Lifecycle.** Documents move through `draft`, `active`, `superseded` and `archived`. A
  `candidate` is `proposed` until `orchy promote` turns it into a real type, or it is
  `rejected`. Promoting a candidate `--as skill` creates a skill from it instead, and the
  candidate stays behind, marked `promoted`, as the record of the proposal.
- **Editing.** `edit` appends to the body by default:
  - `--section` replaces what is under a heading. When several headings share the name,
    orchy refuses (exit 7) until `--nth` picks one;
  - `--replace-in` replaces a piece of text that appears exactly once;
  - `--replace` replaces the whole body.

  Content comes from `--content` or from standard input.
- **Your own fields.** `set` adds any header field you like (`reviewer=alan`,
  `ticket=ORG-42`). Fields orchy manages have their own commands, and `set` names the right
  one when refused: `retitle`, `retype`, `tag`, `ns move`, and `archive`, `unarchive`,
  `supersede` or `promote` for the status. Moving a document to another namespace moves its
  file too.

### Searching

`orchy recall` searches documents and skills together and returns the best matches first:

- words can appear in any order, and each is matched on its own;
- different forms of a word match each other: `migrate`, `migrations` and `migrating` all
  find "migration";
- identifiers are found by their parts: `repository` finds `UserRepository`;
- titles count more than headings, headings more than body text, and the exact phrase you
  typed counts more than the same words scattered;
- recent documents rank above old ones, and `--anchor /backend` prefers results from that
  area without hiding the rest (`--namespace` hides the rest);
- superseded, archived and rejected documents are left out, so replaced knowledge never
  competes with what replaced it. `--status superseded` searches them on purpose.

It matches words, not meaning: `k8s` won't find "kubernetes", typos won't match, and part of
a word (`migr`) finds nothing.

## Skills

A skill is a rule you want every agent to follow. Each has a short `name`, a one-line
`summary` that agents see in their briefing, a body with the detail, and a namespace.

```bash
orchy skill write <name> --summary "…" [--body "…" | --body -] [--namespace /x] [--tag t]...
orchy skill show <name> [--namespace /x]
orchy skill list [--namespace /x] [--tag t]... [--everywhere] [--retired]
orchy skill find <query> [--namespace /x] [--tag t]... [--retired] [--limit n]
orchy skill set <name> [field=value]... [--remove field]... [--tag t]... [--untag t]...
orchy skill retire <name>
orchy skill restore <name>
```

- **Writing and revising.** `write` creates a skill or revises the one with that name in that
  namespace. A new skill needs `--summary`; `--body -` reads the body from standard input.
- **What's in force.** `list` shows the skills in force at a namespace (default `/`), after
  inheritance; `--everywhere` shows all of them.
- **Your own fields.** `set` adds your own header fields.
- **Retiring.** `retire` removes a skill from briefings without deleting it; `restore`
  brings it back.

## Work

```
pending ──claim──▶ claimed ──start──▶ in_progress
   ▲                  │                    │
   └────release───────┴────────────────────┤
                                           ▼
                completed · failed · cancelled · superseded

pending, claimed, in_progress ──block──▶ blocked ──unblock──▶ pending
```

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

- **Taking work.** `task next` hands out the most urgent, then oldest, `pending` task whose
  dependencies are done, and claims it. When several agents ask at once, each gets a
  different task. `--peek` looks without taking. The briefing's "next up" is always the task
  `task next` would hand out.
- **Claims.** A claimed task belongs to its holder until they finish it or release it. Only
  the holder can mark it done, failed or cancelled.
- **Outcomes.** The `--note` of `task done` and the reason given to `task fail` or
  `task cancel` are kept in the task file, under `## Outcome`, for the next agent to read.
- **`split`** breaks a task into subtasks and keeps the original as the goal. The goal
  finishes by itself when its subtasks do:
  - `failed` if any subtask failed;
  - otherwise `completed` if any completed;
  - `superseded` if every subtask was replaced, and `cancelled` otherwise.
- **`replace`** retires a task in favour of new, independent ones.
- **Dependencies** (`--depends-on`, `task dep --add`, `task block --on`) hold a task back
  until every one of them is completed. A dependency that was replaced (`task replace`)
  counts as done once all its replacements are. If a dependency fails or is cancelled, the
  task can never start as planned: `task get` says so, and the briefing flags it to whoever
  holds it. Remove or re-point the dependency with `task dep` to change the plan.

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
| `@name` | that agent, on every machine |
| `@name@machine` | that agent on one machine |
| `role:<role>` | every agent announced with that role |
| `ns:/path` | every agent announced in that area or below |
| `broadcast` | everyone except the sender |

- **Inbox.** `inbox` shows what is new since you last read; `msg read` marks a message read,
  and `--all` shows everything.
- **Roster.** Role and area messages reach agents on the roster, so an agent must have run
  `orchy announce` to receive them.
- **Threads.** `msg resolve` closes a thread.
- **Promoting.** `msg promote` turns a message into a task that links back to it.
- **Message ids.** Message commands take the short id `inbox` prints, or any unique prefix
  or suffix of the full id.

## Links between things

Tasks, documents, skills, messages and agents can be linked with named relations: a task
`produces` a decision, a document `supersedes` an older one, a finding is `supported_by`
evidence. `orchy types` lists every relation and what it can connect.

```bash
orchy link task:<id> document:<id> --rel produces
orchy unlink task:<id> document:<id> --rel produces
orchy graph task:<id> [--depth n]
```

Links take `kind:id` with the full id, where `kind` is `task`, `document`, `skill`,
`message` or `actor`. Relations with side effects are set by their own commands instead:

| relation | set with |
|---|---|
| a subtask's parent | `task update --parent` |
| a dependency | `task dep --add` |
| a replacement | `supersede`, `task replace` |
| a task created from a message | `msg promote` |

## Locks

Stop two agents on the same machine from touching the same thing at once:

```bash
orchy lock acquire <resource> [--ttl secs]    # default 300 seconds; fails if someone else holds it
orchy lock renew <resource> [--ttl secs]
orchy lock release <resource>
orchy lock check <resource>
orchy lock list
orchy lock with <resource> [--ttl secs] -- <command>...   # hold it only while the command runs
```

A resource is any name: a file path, `migrations`, `staging-db`. Locks expire on their own,
so a crashed agent never leaves one stuck.

## History

Every change any agent makes is recorded with who made it and when:

```bash
orchy events [--topic task.] [--key <id>] [--by <agent>] [--limit n]
```

`--topic` matches by prefix: `task.`, `document.`, `skill.`, `message.`, `edge.` (links),
`actor.` (the roster) and `lock.`. `--limit n` shows the `n` most recent.

## Scripting

Every command accepts `--json`. Colour is used only on a terminal, and never when
`NO_COLOR` is set. Exit codes:

| code | meaning |
|---|---|
| 0 | success |
| 2 | bad arguments |
| 4 | not found, or not a vault |
| 5 | refused: held by someone else, not allowed from the current status, or not yours |
| 6 | invalid input |
| 7 | ambiguous: more than one thing matched |
| 8 | storage or I/O failure |

## License

MIT

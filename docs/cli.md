# orchy command reference

Generated from the CLI definition; do not edit by hand. Regenerate with `just cli-doc`.

## `orchy`

orchy stores knowledge, work and conversation as ordinary markdown files in a vault you can read, edit and commit by hand. Agents drive it through this CLI; humans can ignore it and edit the files directly.

```text
orchy [OPTIONS] [COMMAND]
```

- `--vault` Vault directory (default: $ORCHY_VAULT, then settings, then $XDG_DATA_HOME/orchy)
- `--actor` Who is acting: an alias, or alias@machine for a specific instance
- `--session` The session `orchy announce` gave you; it says who you are on every command
- `--json` Emit JSON instead of text
- `--no-color` Never colour the output

## `orchy init`

Create a vault: scaffolds the directories, .gitignore and agent instructions

```text
orchy init [OPTIONS] [PATH]
```

- `<path>`

## `orchy status`

Show the resolved configuration and whether the vault exists

```text
orchy status [OPTIONS]
```

## `orchy announce`

Join the roster and get your briefing: the conventions here, and what is waiting for you

```text
orchy announce [OPTIONS]
```

- `--roles`
- `--namespace`
- `--name`

## `orchy leave`

End this session: its token stops identifying you

```text
orchy leave [OPTIONS]
```

## `orchy guide`

What orchy is and how to drive it, without joining the roster

```text
orchy guide [OPTIONS]
```

## `orchy skill`

Conventions this vault expects every agent to follow

```text
orchy skill [OPTIONS] <COMMAND>
```

## `orchy skill write`

Write a skill down, or revise the one already there

```text
orchy skill write [OPTIONS] <NAME>
```

- `<name>`
- `--summary` The one line every agent reads before deciding to open it
- `--namespace`
- `--body` The skill itself, or `-` to read it from stdin
- `--tag` Cross-cutting label, repeatable
- `--if-match` Refuse unless the skill still hashes to this

## `orchy skill set`

Set any other frontmatter a team wants on a skill

```text
orchy skill set [OPTIONS] <TARGET> [ASSIGNMENTS]...
```

- `<target>`
- `--namespace`
- `<assignments>` field=value, repeatable
- `--remove` Drop a field, repeatable
- `--tag`
- `--untag`
- `--if-match` Refuse unless the skill still hashes to this

## `orchy skill list`

The skills in force where you are working

```text
orchy skill list [OPTIONS]
```

- `--namespace`
- `--tag` Only skills carrying this label, repeatable
- `--everywhere` Every skill in the vault, not only the ones your namespace inherits
- `--retired` Include retired skills

## `orchy skill find`

Match free text against every skill, best first — the way to find one among hundreds

```text
orchy skill find [OPTIONS] [QUERY]...
```

- `<query>`
- `--namespace` Rank skills declared here first
- `--tag`
- `--retired` Search retired skills too
- `--limit`

## `orchy skill show`

Read one, by name or id

```text
orchy skill show [OPTIONS] <TARGET>
```

- `<target>`
- `--namespace`

## `orchy skill retire`

Take a skill out of every briefing without deleting it

```text
orchy skill retire [OPTIONS] <TARGET>
```

- `<target>`

## `orchy skill restore`

Put a retired skill back in force

```text
orchy skill restore [OPTIONS] <TARGET>
```

- `<target>`

## `orchy agents`

List the roster

```text
orchy agents [OPTIONS]
```

- `--live` Only actors seen recently on this machine

## `orchy types`

The registered document types, statuses and relations

```text
orchy types [OPTIONS]
```

## `orchy task`

Work: a task board agents can subdivide

```text
orchy task [OPTIONS] <COMMAND>
```

## `orchy task new`

Create a task

```text
orchy task new [OPTIONS] <TITLE>
```

- `<title>`
- `--description`
- `--acceptance` What must be true for the task to count as done; `-` reads stdin
- `--priority`
- `--namespace`
- `--role`
- `--tag`
- `--parent`
- `--depends-on`

## `orchy task list`

List tasks

```text
orchy task list [OPTIONS]
```

- `--status`
- `--namespace`
- `--blocked` Only work `task next` will not hand out yet, with what each waits on
- `--mine`
- `--role`
- `--parent`
- `--tag`
- `--limit`

## `orchy task get`

Show a task with its subtasks and links

```text
orchy task get [OPTIONS] <TARGET>
```

- `<target>`

## `orchy task ready`

The queue `task next` draws from, in the order it draws

```text
orchy task ready [OPTIONS]
```

- `--namespace`
- `--role`

## `orchy task next`

The highest-ranked claimable task

```text
orchy task next [OPTIONS]
```

- `--role`
- `--namespace`
- `--peek` Look without taking it

## `orchy task claim`

Take a task, with a lease

```text
orchy task claim [OPTIONS] <TARGET>
```

- `<target>`
- `--ttl`
- `--start`

## `orchy task release`

Give a task back

```text
orchy task release [OPTIONS] <TARGET>
```

- `<target>`
- `--force` Take back a task another agent claimed and abandoned; only once its lease expired
- `--reason` Why, recorded with the release

## `orchy task start`

Move a claimed task to in_progress

```text
orchy task start [OPTIONS] <TARGET>
```

- `<target>`

## `orchy task done`

Finish a task; rolls up to the parent

```text
orchy task done [OPTIONS] <TARGET>
```

- `<target>`
- `--note`

## `orchy task fail`

Record a failure; rolls up to the parent

```text
orchy task fail [OPTIONS] <TARGET> <REASON>
```

- `<target>`
- `<reason>`

## `orchy task cancel`

Abandon a task; rolls up to the parent

```text
orchy task cancel [OPTIONS] <TARGET> <REASON>
```

- `<target>`
- `<reason>`

## `orchy task block`

Park a task until something else happens `--on` records a real dependency so the blocker stays queryable; `--reason` covers everything that is not another task

```text
orchy task block [OPTIONS] <TARGET>
```

- `<target>`
- `--on`
- `--reason`

## `orchy task unblock`

Return a blocked task to the pool

```text
orchy task unblock [OPTIONS] <TARGET>
```

- `<target>`

## `orchy task split`

Break a goal into subtasks it waits for

The goal survives as the umbrella: it completes once every subtask reaches a terminal status, and fails if any of them failed. Use `replace` when the original should step aside instead of waiting.

```text
orchy task split [OPTIONS] <TARGET> [TITLES]...
```

- `<target>`
- `<titles>`

## `orchy task replace`

Retire a task, replacing it with independent ones

The original becomes `superseded` and the new tasks stand alone, inheriting whatever goal the original sat under. Use `split` when the original should stay open and wait.

```text
orchy task replace [OPTIONS] <TARGET> [TITLES]...
```

- `<target>`
- `<titles>`
- `--reason`

## `orchy task merge`

Fold duplicates into one task: the others become `superseded`, and their subtasks, tags and dependencies move to the one kept

```text
orchy task merge [OPTIONS] <KEEP> <OTHERS>...
```

- `<keep>`
- `<others>`

## `orchy task dep`

Add or remove dependencies

```text
orchy task dep [OPTIONS] <TARGET>
```

- `<target>`
- `--add`
- `--remove`

## `orchy task update`

Change a task's fields

```text
orchy task update [OPTIONS] <TARGET>
```

- `<target>`
- `--parent` Move this task under another goal
- `--detach` Detach from its current goal
- `--title`
- `--description`
- `--acceptance` What must be true for the task to count as done; `-` reads stdin
- `--priority`
- `--role` Replaces the roles that may claim it, repeatable
- `--namespace`
- `--tag`
- `--untag`

## `orchy msg`

Conversation: a board agents post to

```text
orchy msg [OPTIONS] <COMMAND>
```

## `orchy msg send`

Post a message

```text
orchy msg send [OPTIONS] [TO]...
```

- `<to>` @alias, @alias@machine, role:<r>, ns:<path> or broadcast
- `--subject`
- `--body`
- `--reply-to`
- `--priority`

## `orchy msg inbox`

Everything addressed to you past your read watermark

```text
orchy msg inbox [OPTIONS]
```

- `--all`
- `--thread` Only the conversation this message belongs to

## `orchy msg read`

Show a message and advance the watermark

```text
orchy msg read [OPTIONS] <TARGET>
```

- `<target>`

## `orchy msg thread`

The whole conversation in order

```text
orchy msg thread [OPTIONS] <TARGET>
```

- `<target>`

## `orchy msg sent`

What you have sent

```text
orchy msg sent [OPTIONS]
```

## `orchy msg resolve`

Mark a thread finished

```text
orchy msg resolve [OPTIONS] <TARGET>
```

- `<target>`

## `orchy msg promote`

Turn a message into a task

```text
orchy msg promote [OPTIONS] <TARGET>
```

- `<target>`
- `--title`
- `--role`

## `orchy new`

Create a document

```text
orchy new [OPTIONS] <KIND> <TITLE>
```

- `<kind>`
- `<title>`
- `--namespace`
- `--tag`
- `--body` Body text, `-` for stdin; piped stdin is read when omitted
- `--task` The task whose work produced it

## `orchy read`

Read a document, or one section of it

```text
orchy read [OPTIONS] <TARGET>
```

- `<target>`
- `--section`
- `--nth` Which of several sections sharing the heading (1-based)

## `orchy edit`

Change a document's body

```text
orchy edit [OPTIONS] <TARGET>
```

- `<target>`
- `--section`
- `--nth` Which of several sections sharing the heading (1-based)
- `--replace-in`
- `--replace`
- `--if-match` Refuse the edit unless the document still hashes to this
- `--content` Content; reads stdin when omitted

## `orchy set`

Set an inert frontmatter field

```text
orchy set [OPTIONS] <TARGET> [ASSIGNMENTS]...
```

- `<target>`
- `<assignments>` field=value, repeatable
- `--if-match` Refuse unless the document still hashes to this

## `orchy recall`

Search documents and skills by text, best first

```text
orchy recall [OPTIONS] [QUERY]...
```

- `<query>`
- `--kind`
- `--entity` Look only in `document` or only in `skill`; both by default
- `--status` Only documents with this status; superseded, archived and rejected ones are left out unless asked for
- `--tag`
- `--namespace`
- `--anchor`
- `--limit`
- `--budget` Return the best sections in full, up to about this many tokens
- `--since` Only what changed since a timestamp or within a window: 30m, 2h, 3d, 1w
- `--graph` Also return what the hits link to, up to this many hops away

## `orchy link`

Link two entities with a registered relation

```text
orchy link [OPTIONS] --rel <REL> <FROM> <TO>
```

- `<from>`
- `<to>`
- `--rel`

## `orchy unlink`

Remove a link

```text
orchy unlink [OPTIONS] --rel <REL> <FROM> <TO>
```

- `<from>`
- `<to>`
- `--rel`

## `orchy graph`

Walk the relation graph outward from an entity

```text
orchy graph [OPTIONS] <FROM>
```

- `<from>`
- `--depth`
- `--rel` Follow only these relations, repeatable
- `--format`

## `orchy import`

Create a document from a markdown file, a URL, or `-` for stdin; its frontmatter supplies the title, tags and any other fields

```text
orchy import [OPTIONS] --kind <KIND> <SOURCE>
```

- `<source>`
- `--kind`
- `--title`
- `--namespace`
- `--tag`

## `orchy export`

Print every document, skill, task and message as one JSON object per line

```text
orchy export [OPTIONS]
```

- `--namespace`

## `orchy why`

The story of one entity: what happened to it, by whom, and what it is linked to

```text
orchy why [OPTIONS] <ENTITY>
```

- `<entity>`

## `orchy retitle`

Give a document a new title

```text
orchy retitle [OPTIONS] <TARGET> <TITLE>
```

- `<target>`
- `<title>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy retype`

Change what kind of document it is

```text
orchy retype [OPTIONS] <TARGET> <KIND>
```

- `<target>`
- `<kind>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy tag`

Add or remove tags: `+t` or `t` adds, `-t` removes

```text
orchy tag [OPTIONS] <TARGET> <CHANGES>...
```

- `<target>`
- `<changes>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy ns`

Namespaces

```text
orchy ns [OPTIONS] <COMMAND>
```

## `orchy ns move`

Move a document to another namespace; its file moves with it

```text
orchy ns move [OPTIONS] <TARGET> <NAMESPACE>
```

- `<target>`
- `<namespace>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy supersede`

Mark a document superseded by another

```text
orchy supersede [OPTIONS] --by <BY> <OLD>
```

- `<old>`
- `--by`
- `--if-match` Refuse unless the document still hashes to this

## `orchy consolidate`

Record that duplicates were merged into one document: the sources become superseded by it and their tags carry over. Merge the bodies first, with `edit`

```text
orchy consolidate [OPTIONS] --into <INTO> <SOURCES>...
```

- `<sources>`
- `--into`

## `orchy archive`

Retire a document from active use

```text
orchy archive [OPTIONS] <TARGET>
```

- `<target>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy unarchive`

Bring an archived document back

```text
orchy unarchive [OPTIONS] <TARGET>
```

- `<target>`
- `--if-match` Refuse unless the document still hashes to this

## `orchy promote`

Graduate a candidate into canon as a concrete type

```text
orchy promote [OPTIONS] --as <INTO> <TARGET>
```

- `<target>`
- `--as` What it becomes: decision, pattern, note, … or `skill`
- `--namespace`
- `--name` The skill's name, when promoting into a skill
- `--summary` The skill's one-line summary (default: the candidate's title)
- `--if-match` Refuse unless the document still hashes to this

## `orchy reject`

Turn a candidate down; it stays, marked rejected, out of recall

```text
orchy reject [OPTIONS] <TARGET>
```

- `<target>`
- `--reason`
- `--if-match` Refuse unless the document still hashes to this

## `orchy lock`

Same-machine advisory locks

```text
orchy lock [OPTIONS] <COMMAND>
```

## `orchy lock acquire`

Take a resource, or fail if somebody else holds it

```text
orchy lock acquire [OPTIONS] <RESOURCE>
```

- `<resource>`
- `--ttl` Seconds before the lease lapses on its own (default 300)

## `orchy lock renew`

Extend a lease you already hold, for work that outlives its ttl

```text
orchy lock renew [OPTIONS] <RESOURCE>
```

- `<resource>`
- `--ttl`

## `orchy lock release`

Give a resource back

```text
orchy lock release [OPTIONS] <RESOURCE>
```

- `<resource>`

## `orchy lock check`

Who holds a resource, if anyone

```text
orchy lock check [OPTIONS] <RESOURCE>
```

- `<resource>`

## `orchy lock list`

Every lease still held, across every agent on this machine

```text
orchy lock list [OPTIONS]
```

## `orchy lock with`

Hold a resource for exactly as long as a command runs, and give it back either way

```text
orchy lock with [OPTIONS] <RESOURCE> -- <COMMAND>...
```

- `<resource>`
- `--ttl`
- `<command>` The command to run while holding it

## `orchy events`

Read the event log

```text
orchy events [OPTIONS]
```

- `--topic`
- `--key`
- `--by` Only events by this actor: an alias on any machine, or alias@machine
- `--since` Only events after this: a timestamp, or a window such as 2h or 3d
- `--limit`

## `orchy doctor`

Find what is wrong with the vault; `--fix` repairs what needs no decision

```text
orchy doctor [OPTIONS]
```

- `--fix`

## `orchy completions`

Generate a shell completion script

```text
orchy completions [OPTIONS] <SHELL>
```

- `<shell>`

## `orchy integrate`

Make an agent run `orchy announce` at the start of every session

```text
orchy integrate [OPTIONS] <AGENT>
```

- `<agent>`
- `--dir` The project to set up (default: the current directory)
- `--namespace` Namespace the agent announces itself in
- `--role` Roles the agent announces, repeatable
- `--print` Show the change instead of writing it

## `orchy man`

Print the man page, or write one per command into a directory

```text
orchy man [OPTIONS]
```

- `--out`

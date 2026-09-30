# Changelog

All notable changes to orchy are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). A change that alters how the vault is written is
always called out, because it touches files you commit.

## [Unreleased]

The first release, `0.1.0`, will be cut from this section.

### Added

- **Setup.** `orchy integrate claude-code|codex|opencode|gemini` makes an agent run
  `orchy announce` at the start of every session. Other additions: man pages
  (`orchy man`), a generated command reference in `docs/cli.md`, `ORCHY_NAMESPACE`, and
  release builds with shell and Homebrew installers.
- **Briefing.** A returning agent is told what changed since it last announced. The
  briefing also flags tasks whose dependencies failed and files that could not be read.
- **Tasks.**
  - `task ready` shows the queue, and `task list --blocked` shows what the rest of the board
    waits on.
  - `task new`/`update` accept `--acceptance` and `--role`.
  - `task merge` folds duplicate tasks into one.
  - `task release --force` takes back an abandoned task once its claim's lease has expired.
  - Within a priority, `task next` hands out first the task that other work depends on.
- **Documents.**
  - Commands for single changes: `retitle`, `retype`, `tag`, `ns move`.
  - Candidates can be turned down with `reject`, and `consolidate` records a merge of
    duplicates.
  - `import` creates a document from a file, stdin or a URL, keeping its frontmatter, and
    `export` prints the vault as JSONL.
  - `new --task` links a document to the task that produced it, and `new` points at similar
    documents already in the vault.
  - `read` shows the content hash. Every document and skill change accepts `--if-match`, so
    a change made since you read the file is refused instead of overwritten.
- **Search.**
  - `recall --budget` answers with whole sections up to a token budget.
  - `--status` searches retired documents on purpose, `--since` keeps only recent changes,
    and `--graph` adds what the hits link to.
  - Identifiers are found by their parts (`repository` finds `UserRepository`).
- **Graph and history.**
  - `graph --rel` follows only chosen relations, and `graph --format mermaid|dot` prints a
    diagram.
  - `orchy why` tells one entity's story.
  - Wikilinks to ids (`[[<id>]]`) show up as `mentions`.
  - `events --since` reads recent history.
- **Health.** `orchy doctor` reports what is wrong with a vault, and `--fix` repairs what
  needs no decision: misplaced or misnamed files, stale goals, inverted `supersedes` links.
- **Messages.** `msg inbox --thread` filters the inbox to one conversation, and messages can
  be addressed by short id.
- Listings say when they were cut short.

### Changed

- **Vault format.**
  - A document's folder always equals its namespace. Subfolders a human created beneath a
    namespace are not kept: `doctor` reports such files, and the next save or
    `doctor --fix` moves them to `docs/<namespace>/<id>.md`.
  - `supersedes` is stored on the replacement, pointing at what it replaced; `orchy doctor
    --fix` turns old links around.
  - Files now show what points at them: `superseded_by`, `derives`, `produced_by` and
    `subtasks`.
  - A task keeps its acceptance criteria and its outcome in its body, under `## Acceptance`
    and `## Outcome`.
  - Documents record when they were last updated.
  - Rewrites keep YAML comments, and leave the lines of unchanged fields exactly as written.
- **Statuses.** Documents start `active`, and candidates start `proposed`. Superseded,
  archived and rejected documents are left out of `recall` unless asked for.
- **Skills.** `skill` is a reserved name: candidates are promoted into real skills with
  `promote --as skill`.
- **Dependencies.** A dependency counts as done when it completed, or when it was replaced
  and every replacement completed.
- **Event log.** It now also records links, roster changes, locks and every aggregate
  change.
- **Namespaces.** Agents write where they work, and keep their namespace when they announce
  again.
- **Speed.** Unchanged files are no longer reread on rescan, and searches are scored from
  term counts.

### Fixed

- Reading links (`graph`, `recall --graph`, dependency checks) could let a save overwrite an
  edit another agent made in between; the compare-and-swap now holds.
- A task can no longer be claimed while it waits on an unfinished dependency or has open
  subtasks, and `task next` no longer hands out a goal whose subtasks are still open.
- Superseding, promoting and rejecting a document are final: `archive` then `unarchive` can
  no longer bring a replaced document back.
- `ns move` to a parent namespace moves the file; before, it stayed in the old folder.
- `link` refuses an end that does not exist; `unlink` still removes a dangling link.
- `skill show`, `list` and `set` find the skill in force where the agent works, as the
  briefing does, instead of the root one, and `skill write` without `--namespace` writes
  there too, like every other write.
- Text flags accept values that start with a dash, such as `--acceptance "- works"`.
- `events --by coder-1` matches that alias on every machine.
- Output cut short by the reader (`orchy export | head`) no longer prints a broken-pipe error.
- `doctor` and `recall --graph` are an order of magnitude faster on large vaults.
- A file orchy cannot read is skipped and reported, instead of failing every command.
- A storage failure exits 8 instead of being reported as bad input.
- Links stored on skills appear in the graph.
- Document commands no longer rewrite skills as documents.
- A task's completion note and failure reason survive in its file.
- Section edits refuse an ambiguous heading. A heading inside a code block, or a `#tag`, is
  no longer taken for a section.
- `recall` searches the text before the first heading, and the headings themselves.
- Moving a subtask re-derives its old and new parents, and finishing a goal releases its
  leases.
- `orchy new` reads a piped body, and `orchy guide` works without a vault.
- `events --limit` shows the most recent events.

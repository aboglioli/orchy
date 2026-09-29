use std::path::PathBuf;

use clap::{Parser, Subcommand};

const AFTER_HELP: &str = "\
WHERE THINGS LIVE
  skills     how this team works — binding conventions, inherited down namespaces
  docs       what the team knows — decisions, discoveries, specs, handoffs
  tasks      work with owners and a state machine
  messages   the board every agent posts to

COMMON PATHS
  join              orchy announce
  take work         orchy task next  ->  orchy task done <id> --note ...
  look something up orchy recall <query>  ·  orchy skill show <name>
  write it down     orchy new decision <title>  ·  orchy skill write <name> --summary ...
  say something     orchy msg send broadcast --body ...
  before you stop   orchy new context handoff --body ...

EXIT CODES
  0 ok · 4 not found · 5 refused · 6 bad input · 7 ambiguous · 8 io

`orchy guide` explains the model without joining. `orchy <command> --help` for one command.
";

#[derive(Parser, Debug)]
#[command(
    name = "orchy",
    version,
    before_help = "START HERE: run `orchy announce`. It puts you on the roster and returns the \
                   conventions you are expected to follow and the work waiting for you.",
    about = "A shared, file-backed memory for coding agents",
    long_about = "orchy stores knowledge, work and conversation as ordinary markdown files in a \
                  vault you can read, edit and commit by hand. Agents drive it through this CLI; \
                  humans can ignore it and edit the files directly.",
    after_help = AFTER_HELP
)]
pub(crate) struct Cli {
    /// Vault directory (default: $ORCHY_VAULT, then settings, then $XDG_DATA_HOME/orchy)
    #[arg(long, global = true, env = "ORCHY_VAULT")]
    pub vault: Option<PathBuf>,

    /// Who is acting: an alias, or alias@machine for a specific instance
    #[arg(long, global = true, env = "ORCHY_ACTOR")]
    pub actor: Option<String>,

    /// Emit JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,

    /// Never colour the output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Optional so that a bare `orchy` prints the guidance rather than clap's one-line
    /// refusal, which is all an agent would otherwise get once ORCHY_VAULT is set.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Create a vault: scaffolds the directories, .gitignore and agent instructions
    Init { path: Option<PathBuf> },
    /// Show the resolved configuration and whether the vault exists
    Status,
    /// Join the roster and get your briefing: the conventions here, and what is waiting for you
    Announce {
        #[arg(long)]
        roles: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// What orchy is and how to drive it, without joining the roster
    Guide,
    /// Conventions this vault expects every agent to follow
    #[command(subcommand)]
    Skill(SkillCommand),
    /// List the roster
    Agents {
        /// Only actors seen recently on this machine
        #[arg(long)]
        live: bool,
    },
    /// The registered document types, statuses and relations
    Types,
    /// Work: a task board agents can subdivide
    #[command(subcommand)]
    Task(TaskCommand),
    /// Conversation: a board agents post to
    #[command(subcommand)]
    Msg(MsgCommand),
    /// Create a document
    New {
        kind: String,
        title: String,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// Body text, `-` for stdin; piped stdin is read when omitted
        #[arg(long)]
        body: Option<String>,
        /// The task whose work produced it
        #[arg(long)]
        task: Option<String>,
    },
    /// Read a document, or one section of it
    Read {
        target: String,
        #[arg(long)]
        section: Option<String>,
        /// Which of several sections sharing the heading (1-based)
        #[arg(long, requires = "section")]
        nth: Option<usize>,
    },
    /// Change a document's body
    Edit {
        target: String,
        #[arg(long)]
        section: Option<String>,
        /// Which of several sections sharing the heading (1-based)
        #[arg(long, requires = "section")]
        nth: Option<usize>,
        #[arg(long)]
        replace_in: Option<String>,
        #[arg(long)]
        replace: bool,
        /// Refuse the edit unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
        /// Content; reads stdin when omitted
        #[arg(long)]
        content: Option<String>,
    },
    /// Set an inert frontmatter field
    Set {
        target: String,
        /// field=value, repeatable
        assignments: Vec<String>,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Search documents and skills by text, best first
    Recall {
        query: Vec<String>,
        #[arg(long)]
        kind: Vec<String>,
        /// Look only in `document` or only in `skill`; both by default
        #[arg(long = "entity")]
        entities: Vec<String>,
        /// Only documents with this status; superseded, archived and rejected ones are
        /// left out unless asked for
        #[arg(long)]
        status: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        anchor: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        /// Return the best sections in full, up to about this many tokens
        #[arg(long)]
        budget: Option<usize>,
        /// Only what changed since a timestamp or within a window: 30m, 2h, 3d, 1w
        #[arg(long)]
        since: Option<String>,
        /// Also return what the hits link to, up to this many hops away
        #[arg(long, default_value_t = 0)]
        graph: u8,
    },
    /// Link two entities with a registered relation
    Link {
        from: String,
        to: String,
        #[arg(long)]
        rel: String,
    },
    /// Remove a link
    Unlink {
        from: String,
        to: String,
        #[arg(long)]
        rel: String,
    },
    /// Walk the relation graph outward from an entity
    Graph {
        from: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        /// Follow only these relations, repeatable
        #[arg(long)]
        rel: Vec<String>,
        #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
        format: GraphFormat,
    },
    /// The story of one entity: what happened to it, by whom, and what it is linked to
    Why { entity: String },
    /// Give a document a new title
    Retitle {
        target: String,
        title: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Change what kind of document it is
    Retype {
        target: String,
        kind: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Add or remove tags: `+t` or `t` adds, `-t` removes
    Tag {
        target: String,
        #[arg(allow_hyphen_values = true, required = true)]
        changes: Vec<String>,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Namespaces
    #[command(subcommand)]
    Ns(NsCommand),
    /// Mark a document superseded by another
    Supersede {
        old: String,
        #[arg(long)]
        by: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Record that duplicates were merged into one document: the sources become superseded
    /// by it and their tags carry over. Merge the bodies first, with `edit`.
    Consolidate {
        #[arg(required = true)]
        sources: Vec<String>,
        #[arg(long)]
        into: String,
    },
    /// Retire a document from active use
    Archive {
        target: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Bring an archived document back
    Unarchive {
        target: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Graduate a candidate into canon as a concrete type
    Promote {
        target: String,
        /// What it becomes: decision, pattern, note, … or `skill`
        #[arg(long = "as")]
        into: String,
        #[arg(long)]
        namespace: Option<String>,
        /// The skill's name, when promoting into a skill
        #[arg(long)]
        name: Option<String>,
        /// The skill's one-line summary (default: the candidate's title)
        #[arg(long)]
        summary: Option<String>,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Turn a candidate down; it stays, marked rejected, out of recall
    Reject {
        target: String,
        #[arg(long)]
        reason: Option<String>,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Same-machine advisory locks
    #[command(subcommand)]
    Lock(LockCommand),
    /// Read the event log
    Events {
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        key: Option<String>,
        /// Only events recorded by this actor id
        #[arg(long = "by")]
        by: Option<String>,
        /// Only events after this: a timestamp, or a window such as 2h or 3d
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Find what is wrong with the vault; `--fix` repairs what needs no decision
    Doctor {
        #[arg(long)]
        fix: bool,
    },
    /// Generate a shell completion script
    Completions { shell: clap_complete::Shell },
    /// Make an agent run `orchy announce` at the start of every session
    Integrate {
        agent: crate::integrate::Agent,
        /// The project to set up (default: the current directory)
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Namespace the agent announces itself in
        #[arg(long)]
        namespace: Option<String>,
        /// Roles the agent announces, repeatable
        #[arg(long)]
        role: Vec<String>,
        /// Show the change instead of writing it
        #[arg(long)]
        print: bool,
    },
    /// Print the man page, or write one per command into a directory
    Man {
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum TaskCommand {
    /// Create a task
    New {
        title: String,
        #[arg(long)]
        description: Option<String>,
        /// What must be true for the task to count as done; `-` reads stdin
        #[arg(long)]
        acceptance: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        role: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        depends_on: Vec<String>,
    },
    /// List tasks
    List {
        #[arg(long)]
        status: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        /// Only work `task next` will not hand out yet, with what each waits on
        #[arg(long, conflicts_with_all = ["status", "mine", "role", "parent", "tag"])]
        blocked: bool,
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show a task with its subtasks and links
    Get { target: String },
    /// The queue `task next` draws from, in the order it draws
    Ready {
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// The highest-ranked claimable task
    Next {
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
        /// Look without taking it
        #[arg(long)]
        peek: bool,
    },
    /// Take a task, with a lease
    Claim {
        target: String,
        #[arg(long)]
        ttl: Option<i64>,
        #[arg(long)]
        start: bool,
    },
    /// Give a task back
    Release {
        target: String,
        /// Take back a task another agent claimed and abandoned; only once its lease expired
        #[arg(long, requires = "reason")]
        force: bool,
        /// Why, recorded with the release
        #[arg(long)]
        reason: Option<String>,
    },
    /// Move a claimed task to in_progress
    Start { target: String },
    /// Finish a task; rolls up to the parent
    Done {
        target: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Record a failure; rolls up to the parent
    Fail { target: String, reason: String },
    /// Abandon a task; rolls up to the parent
    Cancel { target: String, reason: String },
    /// Park a task until something else happens
    /// `--on` records a real dependency so the blocker stays queryable; `--reason` covers
    /// everything that is not another task.
    Block {
        target: String,
        #[arg(long)]
        on: Vec<String>,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Return a blocked task to the pool
    Unblock { target: String },
    /// Break a goal into subtasks it waits for
    ///
    /// The goal survives as the umbrella: it completes once every subtask reaches a terminal
    /// status, and fails if any of them failed. Use `replace` when the original should step
    /// aside instead of waiting.
    Split { target: String, titles: Vec<String> },
    /// Retire a task, replacing it with independent ones
    ///
    /// The original becomes `superseded` and the new tasks stand alone, inheriting whatever
    /// goal the original sat under. Use `split` when the original should stay open and wait.
    Replace {
        target: String,
        titles: Vec<String>,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Fold duplicates into one task: the others become `superseded`, and their subtasks,
    /// tags and dependencies move to the one kept
    Merge {
        keep: String,
        #[arg(required = true)]
        others: Vec<String>,
    },
    /// Add or remove dependencies
    Dep {
        target: String,
        #[arg(long)]
        add: Vec<String>,
        #[arg(long)]
        remove: Vec<String>,
    },
    /// Change a task's fields
    Update {
        target: String,
        /// Move this task under another goal
        #[arg(long)]
        parent: Option<String>,
        /// Detach from its current goal
        #[arg(long, conflicts_with = "parent")]
        detach: bool,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// What must be true for the task to count as done; `-` reads stdin
        #[arg(long)]
        acceptance: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        /// Replaces the roles that may claim it, repeatable
        #[arg(long)]
        role: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        untag: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum MsgCommand {
    /// Post a message
    Send {
        /// @alias, @alias@machine, role:<r>, ns:<path> or broadcast
        to: Vec<String>,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        reply_to: Option<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    /// Everything addressed to you past your read watermark
    Inbox {
        #[arg(long)]
        all: bool,
        /// Only the conversation this message belongs to
        #[arg(long)]
        thread: Option<String>,
    },
    /// Show a message and advance the watermark
    Read { target: String },
    /// The whole conversation in order
    Thread { target: String },
    /// What you have sent
    Sent,
    /// Mark a thread finished
    Resolve { target: String },
    /// Turn a message into a task
    Promote {
        target: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        role: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum SkillCommand {
    /// Write a skill down, or revise the one already there
    Write {
        name: String,
        /// The one line every agent reads before deciding to open it
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        namespace: Option<String>,
        /// The skill itself, or `-` to read it from stdin
        #[arg(long)]
        body: Option<String>,
        /// Cross-cutting label, repeatable
        #[arg(long)]
        tag: Vec<String>,
        /// Refuse unless the skill still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
    /// Set any other frontmatter a team wants on a skill
    Set {
        target: String,
        #[arg(long)]
        namespace: Option<String>,
        #[command(flatten)]
        edits: SkillEdits,
    },
    /// The skills in force where you are working
    List {
        #[arg(long)]
        namespace: Option<String>,
        /// Only skills carrying this label, repeatable
        #[arg(long)]
        tag: Vec<String>,
        /// Every skill in the vault, not only the ones your namespace inherits
        #[arg(long)]
        everywhere: bool,
        /// Include retired skills
        #[arg(long)]
        retired: bool,
    },
    /// Match free text against every skill, best first — the way to find one among hundreds
    Find {
        query: Vec<String>,
        /// Rank skills declared here first
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// Search retired skills too
        #[arg(long)]
        retired: bool,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Read one, by name or id
    Show {
        target: String,
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Take a skill out of every briefing without deleting it
    Retire { target: String },
    /// Put a retired skill back in force
    Restore { target: String },
}

#[derive(clap::Args, Debug)]
pub(crate) struct SkillEdits {
    /// field=value, repeatable
    pub assignments: Vec<String>,
    /// Drop a field, repeatable
    #[arg(long)]
    pub remove: Vec<String>,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub untag: Vec<String>,
    /// Refuse unless the skill still hashes to this
    #[arg(long)]
    pub if_match: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum GraphFormat {
    Text,
    Mermaid,
    Dot,
}

#[derive(Subcommand, Debug)]
pub(crate) enum NsCommand {
    /// Move a document to another namespace; its file moves with it
    Move {
        target: String,
        namespace: String,
        /// Refuse unless the document still hashes to this
        #[arg(long)]
        if_match: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum LockCommand {
    /// Take a resource, or fail if somebody else holds it
    Acquire {
        resource: String,
        /// Seconds before the lease lapses on its own (default 300)
        #[arg(long)]
        ttl: Option<i64>,
    },
    /// Extend a lease you already hold, for work that outlives its ttl
    Renew {
        resource: String,
        #[arg(long)]
        ttl: Option<i64>,
    },
    /// Give a resource back
    Release { resource: String },
    /// Who holds a resource, if anyone
    Check { resource: String },
    /// Every lease still held, across every agent on this machine
    List,
    /// Hold a resource for exactly as long as a command runs, and give it back either way
    With {
        resource: String,
        #[arg(long)]
        ttl: Option<i64>,
        /// The command to run while holding it
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

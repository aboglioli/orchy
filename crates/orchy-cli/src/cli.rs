use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "orchy",
    version,
    about = "A shared, file-backed memory for coding agents",
    long_about = "orchy stores knowledge, work and conversation as ordinary markdown files in a \
                  vault you can read, edit and commit by hand. Agents drive it through this CLI; \
                  humans can ignore it and edit the files directly."
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

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Create a vault: scaffolds the directories, .gitignore and agent instructions
    Init { path: Option<PathBuf> },
    /// Show the resolved configuration and whether the vault exists
    Status,
    /// Join the roster and refresh presence
    Announce {
        #[arg(long)]
        roles: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
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
        /// Body text; reads stdin when omitted
        #[arg(long)]
        body: Option<String>,
    },
    /// Read a document, or one section of it
    Read {
        target: String,
        #[arg(long)]
        section: Option<String>,
    },
    /// Change a document's body
    Edit {
        target: String,
        #[arg(long)]
        section: Option<String>,
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
    },
    /// Search document sections, ranked
    Recall {
        query: Vec<String>,
        #[arg(long)]
        kind: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        anchor: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
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
    },
    /// Mark a document superseded by another
    Supersede {
        old: String,
        #[arg(long)]
        by: String,
    },
    /// Promote a candidate into canon
    Promote {
        target: String,
        #[arg(long)]
        namespace: Option<String>,
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
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Generate a shell completion script
    Completions { shell: clap_complete::Shell },
}

#[derive(Subcommand, Debug)]
pub(crate) enum TaskCommand {
    /// Create a task
    New {
        title: String,
        #[arg(long)]
        description: Option<String>,
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
    Release { target: String },
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
    Block { target: String, reason: String },
    /// Return a blocked task to the pool
    Unblock { target: String },
    /// Break a goal into subtasks
    Split { target: String, titles: Vec<String> },
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
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<String>,
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
pub(crate) enum LockCommand {
    Acquire {
        resource: String,
        #[arg(long)]
        ttl: Option<i64>,
    },
    Release {
        resource: String,
    },
    Check {
        resource: String,
    },
}

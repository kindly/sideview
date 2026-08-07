mod cli;
mod daemon;
mod diff;
mod format;
mod netcheck;
mod render;
mod session;
mod skill;
mod store;

use clap::{Args, Parser, Subcommand};

/// A visual side channel for CLI agents.
///
/// Bare `sideview` ensures this project's daemon is up, then opens the page.
#[derive(Parser)]
#[command(name = "sideview", version, arg_required_else_help = false)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Run the daemon detached instead of in the foreground
    #[arg(long)]
    detach: bool,

    /// What to bind: `auto` (loopback + tailnet when there is one) or `loopback`
    #[arg(long, default_value = "auto", env = "SIDEVIEW_BIND")]
    bind: String,
}

#[derive(Args)]
struct AuthorArgs {
    /// Explicit session id (otherwise resolved from the environment)
    #[arg(long)]
    session: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Append a markdown block (content on stdin); prints the block id
    Prose(AuthorArgs),
    /// Append an HTML fragment, styled by the page (stdin); prints the block id
    Markup(AuthorArgs),
    /// Append a whole HTML document, isolated in an iframe (stdin); prints the block id
    Html {
        /// Iframe height as a CSS length (e.g. 40rem, 600px); viewers can still drag
        #[arg(long)]
        height: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Append a unified diff (stdin — e.g. `git diff | sideview diff`); prints the block id
    Diff(AuthorArgs),
    /// Replace a block's content in place (new content on stdin)
    Update {
        /// The id an authoring command printed, e.g. b7
        id: String,
        /// Content type of the replacement (defaults to prose)
        #[arg(long, default_value = "prose")]
        r#type: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Remove a block from the page
    Rm {
        /// The id an authoring command printed, e.g. b7
        id: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Set properties of a page, delete one, or promote it into the repo
    Page {
        #[command(subcommand)]
        action: PageCmd,
    },
    /// Alias of `page` — lives one release, then goes
    Session {
        #[command(subcommand)]
        action: SessionCmd,
    },
    /// Bind a committed .sv file as a page (the missing verb for documents
    /// that live in the repo rather than under .sideview/pages/)
    Open {
        /// The .sv file, relative to the project
        file: std::path::PathBuf,
    },
    /// Comment on a block (body on stdin): --at places it, --thread replies
    Comment {
        /// Target block id (e.g. b3); omit when replying with --thread
        block: Option<String>,
        /// Anchor within the block (h:…, p:…, l:…); absent = the block's tail
        #[arg(long)]
        at: Option<String>,
        /// Reply to an existing thread instead of starting one
        #[arg(long)]
        thread: Option<i64>,
        /// Page (binding id) to comment on; otherwise resolved from the environment
        #[arg(long)]
        page: Option<String>,
    },
    /// Resolve a thread (--undo reopens). Normally the user's move — agents
    /// resolve only when asked; answering a thread is not closing it
    Resolve {
        /// The thread id (watch events carry it)
        thread: i64,
        /// Reopen instead: clears resolved_at, the thread reattaches
        #[arg(long)]
        undo: bool,
    },
    /// Give the rail an explicit outline (JSON entries on stdin); used verbatim
    Outline {
        /// Remove the explicit outline; the rail returns to prose derivation
        #[arg(long)]
        clear: bool,
        /// Page (binding id); otherwise resolved from the environment
        #[arg(long)]
        page: Option<String>,
    },
    /// Await feedback: typed JSON-lines (comment/resolve/unresolve) on stdout
    Watch {
        /// Give up quietly after this many seconds
        #[arg(long)]
        timeout: Option<u64>,
        /// Also emit comments back to this id (watch starts at invocation otherwise)
        #[arg(long)]
        since: Option<i64>,
        /// Claim each comment (exactly-once across concurrent watchers)
        #[arg(long)]
        claim: bool,
    },
    /// What's running here, and at which URLs
    Sessions,
    /// Is the daemon alive, on which port, at which version
    Status,
    /// Print the class vocabulary
    Styles,
    /// Delete the store and start again (pre-1.0 escape hatch)
    Reset,
    /// Install or remove the agent skill
    Skill {
        #[command(subcommand)]
        action: SkillCmd,
    },
    /// Run the daemon in this process (internal; used by auto-start)
    #[command(name = "__daemon", hide = true)]
    InternalDaemon {
        /// Open the browser once ready
        #[arg(long)]
        open: bool,
        #[arg(long, default_value = "auto")]
        bind: String,
    },
}

#[derive(Subcommand)]
enum PageCmd {
    /// Set page properties: a human-facing label, whether the outline shows
    Set {
        /// Name shown in the page's session strip
        #[arg(long)]
        label: Option<String>,
        /// Contents rail: `scrollspy` (default — whole page, rail follows the
        /// scroll), `tabs` (sections as separate panes), or `off`
        #[arg(long)]
        outline: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Delete a page: its file, its binding, its conversation. No id means this session's
    Rm {
        /// The page to delete (defaults to your own)
        id: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Move a throwaway page into the repo, the binding following
    Promote {
        /// Destination path for the .sv file, relative to the project
        dest: std::path::PathBuf,
        #[command(flatten)]
        author: AuthorArgs,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    /// Set page properties: a human-facing label, whether the outline shows
    Set {
        /// Name shown in the page's session strip
        #[arg(long)]
        label: Option<String>,
        /// Contents rail: `scrollspy` (default — whole page, rail follows the
        /// scroll), `tabs` (sections as separate panes), or `off`
        #[arg(long)]
        outline: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Delete a page: its file and its binding. No id means this session's
    Rm {
        /// The session to delete (defaults to your own)
        id: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Write the embedded skill to every present harness's skills dir
    /// (claude, codex, opencode, pi)
    Install {
        /// Write to ./.claude/skills/ instead, for committing to the repo
        #[arg(long)]
        project: bool,
        /// Only this harness: claude, codex, opencode or pi
        #[arg(long)]
        agent: Option<String>,
    },
    /// Remove the installed skill
    Uninstall {
        #[arg(long)]
        project: bool,
        /// Only this harness: claude, codex, opencode or pi
        #[arg(long)]
        agent: Option<String>,
    },
}

fn kind(name: &str) -> anyhow::Result<cli::Kind> {
    match name {
        "prose" => Ok(cli::Kind::Prose),
        "markup" => Ok(cli::Kind::Markup),
        "html" => Ok(cli::Kind::Html),
        "diff" => Ok(cli::Kind::Diff),
        other => anyhow::bail!("unknown block type {other:?} (prose, markup, html or diff)"),
    }
}

fn main() -> anyhow::Result<()> {
    // Rust ignores SIGPIPE, so `sideview status | head` would panic on EPIPE
    // mid-print. Restore the default: die quietly like every other CLI.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args = Cli::parse();
    match args.cmd {
        // A tool that does the useful thing beats one that lectures you.
        None => cli::open(args.detach, &args.bind),
        Some(Cmd::Prose(a)) => cli::author(cli::Kind::Prose, a.session.as_deref(), &[]),
        Some(Cmd::Markup(a)) => cli::author(cli::Kind::Markup, a.session.as_deref(), &[]),
        Some(Cmd::Html { height, author }) => {
            let extra: Vec<(&str, &str)> =
                height.as_deref().map(|h| ("height", h)).into_iter().collect();
            cli::author(cli::Kind::Html, author.session.as_deref(), &extra)
        }
        Some(Cmd::Diff(a)) => cli::author(cli::Kind::Diff, a.session.as_deref(), &[]),
        Some(Cmd::Update { id, r#type, author }) => {
            cli::update(&id, kind(&r#type)?, author.session.as_deref())
        }
        Some(Cmd::Rm { id, author }) => cli::rm(&id, author.session.as_deref()),
        Some(Cmd::Page { action: PageCmd::Set { label, outline, author } })
        | Some(Cmd::Session { action: SessionCmd::Set { label, outline, author } }) => {
            cli::session_set(author.session.as_deref(), label.as_deref(), outline.as_deref())
        }
        Some(Cmd::Page { action: PageCmd::Rm { id, author } })
        | Some(Cmd::Session { action: SessionCmd::Rm { id, author } }) => {
            cli::session_rm(author.session.as_deref(), id.as_deref())
        }
        Some(Cmd::Page { action: PageCmd::Promote { dest, author } }) => {
            cli::page_promote(author.session.as_deref(), &dest)
        }
        Some(Cmd::Open { file }) => cli::open_page(&file),
        Some(Cmd::Comment { block, at, thread, page }) => {
            cli::comment(block.as_deref(), at.as_deref(), thread, page.as_deref())
        }
        Some(Cmd::Resolve { thread, undo }) => cli::resolve(thread, undo),
        Some(Cmd::Outline { clear, page }) => cli::outline(clear, page.as_deref()),
        Some(Cmd::Watch { timeout, since, claim }) => cli::watch(timeout, since, claim),
        Some(Cmd::Sessions) => cli::sessions(),
        Some(Cmd::Status) => cli::status(),
        Some(Cmd::Styles) => cli::styles(),
        Some(Cmd::Reset) => cli::reset(),
        Some(Cmd::Skill { action: SkillCmd::Install { project, agent } }) => {
            skill::install(project, agent.as_deref())
        }
        Some(Cmd::Skill { action: SkillCmd::Uninstall { project, agent } }) => {
            skill::uninstall(project, agent.as_deref())
        }
        Some(Cmd::InternalDaemon { open, bind }) => {
            let cwd = std::env::current_dir()?;
            let dir = store::find_store_dir(&cwd);
            daemon::run(
                &dir,
                &daemon::Opts { bind_auto: bind != "loopback", open_browser: open },
            )
        }
    }
}

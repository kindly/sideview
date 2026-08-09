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

    /// Pin the port (agents: export SIDEVIEW_PORT once and every spawn,
    /// auto-spawns included, inherits it). Unset: last port, else ephemeral
    #[arg(long, env = "SIDEVIEW_PORT")]
    port: Option<u16>,

    /// Resolve the store from this project directory instead of the cwd
    /// (agents with resetting shells: this or SIDEVIEW_PROJECT beats cd)
    #[arg(long, global = true, env = "SIDEVIEW_PROJECT")]
    project: Option<std::path::PathBuf>,
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
        /// Guard: refuse unless the thread is on this page (events carry
        /// both — asserting the pair catches cross-project id collisions)
        #[arg(long)]
        page: Option<String>,
    },
    /// Mark a thread as being worked on (for tasks that will take a while;
    /// quick replies don't need it). Cleared by your next reply or a resolve
    Working {
        /// The thread id (watch events carry it)
        thread: i64,
        /// Guard: refuse unless the thread is on this page
        #[arg(long)]
        page: Option<String>,
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
        /// Claim each comment (exactly-once across concurrent watchers).
        /// Claim only what you will act on: a claimed event lost in transit
        /// is invisible to reach-back
        #[arg(long)]
        claim: bool,
        /// Drop events authored by this role (e.g. your own echoes: agent).
        /// Server-side, so a comment merely quoting the pattern survives
        #[arg(long)]
        skip_author: Option<String>,
        /// Stamp each emitted comment as seen (a delivery receipt the page
        /// shows as "seen" while the agent works — receipt, not cognition,
        /// and unlike --claim it never suppresses emission)
        #[arg(long)]
        ack: bool,
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
    /// Comment-attachment housekeeping
    Attachments {
        #[command(subcommand)]
        action: AttachmentsCmd,
    },
    /// Run the daemon in this process (internal; used by auto-start)
    #[command(name = "__daemon", hide = true)]
    InternalDaemon {
        /// Open the browser once ready
        #[arg(long)]
        open: bool,
        #[arg(long, default_value = "auto")]
        bind: String,
        #[arg(long, env = "SIDEVIEW_PORT")]
        port: Option<u16>,
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
enum AttachmentsCmd {
    /// Collect attachment files nothing references — checked against live
    /// conversation and every current page's content. Deliberate, never a
    /// daemon habit; its writ never leaves .sideview/attachments/
    Gc {
        /// Also collect attachments held only by resolved threads (their
        /// rows go with the files). Never the default: resolved
        /// conversation is folded, not gone
        #[arg(long)]
        resolved: bool,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Write the embedded skill to every present harness's skills dir
    /// (claude, codex, opencode, pi)
    Install {
        /// Write to ./.claude/skills/ instead, for committing to the repo
        /// (`--repo`, not `--project` — the global `--project <dir>` owns that name,
        /// and clap can't hold two arg ids of different types; 0.2.0 panicked here)
        #[arg(long)]
        repo: bool,
        /// Only this harness: claude, codex, opencode or pi
        #[arg(long)]
        agent: Option<String>,
    },
    /// Remove the installed skill
    Uninstall {
        #[arg(long)]
        repo: bool,
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
    if let Some(dir) = &args.project {
        // The one honest way to make every store-resolution below agree:
        // become the project. Fails loudly if it isn't a directory.
        std::env::set_current_dir(dir)
            .map_err(|e| anyhow::anyhow!("--project {}: {e}", dir.display()))?;
    }
    match args.cmd {
        // A tool that does the useful thing beats one that lectures you.
        None => cli::open(args.detach, &args.bind, args.port),
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
        Some(Cmd::Resolve { thread, undo, page }) => cli::resolve(thread, undo, page.as_deref()),
        Some(Cmd::Working { thread, page }) => cli::working(thread, page.as_deref()),
        Some(Cmd::Outline { clear, page }) => cli::outline(clear, page.as_deref()),
        Some(Cmd::Watch { timeout, since, claim, skip_author, ack }) => {
            cli::watch(timeout, since, claim, skip_author.as_deref(), ack)
        }
        Some(Cmd::Sessions) => cli::sessions(),
        Some(Cmd::Status) => cli::status(),
        Some(Cmd::Styles) => cli::styles(),
        Some(Cmd::Reset) => cli::reset(),
        Some(Cmd::Attachments { action: AttachmentsCmd::Gc { resolved } }) => {
            cli::attachments_gc(resolved)
        }
        Some(Cmd::Skill { action: SkillCmd::Install { repo, agent } }) => {
            skill::install(repo, agent.as_deref())
        }
        Some(Cmd::Skill { action: SkillCmd::Uninstall { repo, agent } }) => {
            skill::uninstall(repo, agent.as_deref())
        }
        Some(Cmd::InternalDaemon { open, bind, port }) => {
            let cwd = std::env::current_dir()?;
            let dir = store::find_store_dir(&cwd);
            daemon::run(
                &dir,
                &daemon::Opts { bind_auto: bind != "loopback", open_browser: open, port },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 0.2.0 shipped a panic: the global `--project <dir>` and `skill install
    // --project` (bool) shared the clap id `project`, globals propagate into
    // every subcommand, and clap dies on the typed access. debug_assert pins
    // the structural rule for every future arg; the parses pin the exact
    // invocations that died.
    #[test]
    fn no_arg_id_collides_with_a_global() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
        Cli::try_parse_from(["sideview", "skill", "install", "--agent", "claude"]).unwrap();
        Cli::try_parse_from(["sideview", "skill", "install"]).unwrap();
        Cli::try_parse_from(["sideview", "--project", "/tmp", "skill", "install", "--repo"]).unwrap();
    }
}

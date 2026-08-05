mod cli;
mod daemon;
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
    Html(AuthorArgs),
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
    /// Set properties of this session's page
    Session {
        #[command(subcommand)]
        action: SessionCmd,
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
    /// Write the embedded skill to ~/.claude/skills/sideview/
    Install {
        /// Write to ./.claude/skills/ instead, for committing to the repo
        #[arg(long)]
        project: bool,
    },
    /// Remove the installed skill
    Uninstall {
        #[arg(long)]
        project: bool,
    },
}

fn kind(name: &str) -> anyhow::Result<cli::Kind> {
    match name {
        "prose" => Ok(cli::Kind::Prose),
        "markup" => Ok(cli::Kind::Markup),
        "html" => Ok(cli::Kind::Html),
        other => anyhow::bail!("unknown block type {other:?} (prose, markup or html)"),
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
        Some(Cmd::Prose(a)) => cli::author(cli::Kind::Prose, a.session.as_deref()),
        Some(Cmd::Markup(a)) => cli::author(cli::Kind::Markup, a.session.as_deref()),
        Some(Cmd::Html(a)) => cli::author(cli::Kind::Html, a.session.as_deref()),
        Some(Cmd::Update { id, r#type, author }) => {
            cli::update(&id, kind(&r#type)?, author.session.as_deref())
        }
        Some(Cmd::Rm { id, author }) => cli::rm(&id, author.session.as_deref()),
        Some(Cmd::Session { action: SessionCmd::Set { label, outline, author } }) => {
            cli::session_set(author.session.as_deref(), label.as_deref(), outline.as_deref())
        }
        Some(Cmd::Session { action: SessionCmd::Rm { id, author } }) => {
            cli::session_rm(author.session.as_deref(), id.as_deref())
        }
        Some(Cmd::Sessions) => cli::sessions(),
        Some(Cmd::Status) => cli::status(),
        Some(Cmd::Styles) => cli::styles(),
        Some(Cmd::Reset) => cli::reset(),
        Some(Cmd::Skill { action: SkillCmd::Install { project } }) => skill::install(project),
        Some(Cmd::Skill { action: SkillCmd::Uninstall { project } }) => skill::uninstall(project),
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

//! Everything the CLI does apart from being the daemon. Authoring commands
//! return immediately; only bare `sideview` ever waits, because only it needs
//! a URL to open a browser at.

use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::daemon;
use crate::netcheck;
use crate::session;
use crate::spec::Spec;
use crate::store::{self, Store, SPAWN_LOCK};

pub enum Kind {
    Prose,
    Markup,
    Html,
}

impl Kind {
    fn spec(&self, content: String) -> Spec {
        match self {
            Kind::Prose => Spec::Prose { text: content },
            Kind::Markup => Spec::Markup { html: content },
            Kind::Html => Spec::Html { document: content },
        }
    }
}

fn open_project_store() -> Result<Store> {
    let cwd = std::env::current_dir()?;
    Store::open(&store::find_store_dir(&cwd))
}

fn read_stdin() -> Result<String> {
    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;
    Ok(content)
}

fn resolve_and_touch(store: &Store, explicit: Option<&str>) -> Result<String> {
    let cwd = std::env::current_dir()?;
    let resolved = session::resolve(explicit, &cwd);
    store.touch_session(&resolved.id, &cwd.display().to_string(), resolved.detected_from)?;
    Ok(resolved.id)
}

/// `sideview prose|markup|html` — write the block, print its id alone on
/// stdout, deal with the daemon afterwards, exit without waiting.
pub fn author(kind: Kind, explicit_session: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let session_id = resolve_and_touch(&store, explicit_session)?;
    let spec = kind.spec(read_stdin()?);
    let short_id = store.insert_block(&session_id, &spec)?;
    println!("{short_id}");
    ensure_daemon(&mut store)?;
    Ok(())
}

pub fn update(short_id: &str, kind: Kind, explicit_session: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let session_id = resolve_and_touch(&store, explicit_session)?;
    store.update_block(&session_id, short_id, &kind.spec(read_stdin()?))?;
    ensure_daemon(&mut store)?;
    Ok(())
}

pub fn rm(short_id: &str, explicit_session: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let session_id = resolve_and_touch(&store, explicit_session)?;
    store.rm_block(&session_id, short_id)?;
    ensure_daemon(&mut store)?;
    Ok(())
}

/// After a write: if a reachable daemon is alive, nothing to do (bar the
/// version-skew line). Otherwise auto-spawn unless this namespace is provably
/// unreachable, in which case print the one line that fixes it. The block is
/// already written either way — nothing is ever lost.
fn ensure_daemon(store: &mut Store) -> Result<()> {
    if let Some(d) = store.daemon_alive()? {
        if d.reachable {
            if d.version != env!("CARGO_PKG_VERSION") {
                eprintln!(
                    "daemon is running v{}, this is v{} — Ctrl-C it and run `sideview` again",
                    d.version,
                    env!("CARGO_PKG_VERSION")
                );
            }
            return Ok(());
        }
        // Alive but namespaced: useless to any browser. Fall through — a
        // spawn from a reachable namespace will claim the row and the old
        // daemon evicts itself.
    }
    let verdict = netcheck::verdict();
    if !verdict.reachable {
        eprintln!(
            "no daemon running — run `sideview` in {} to see this.",
            store.root.display()
        );
        return Ok(());
    }
    spawn_detached(store, /* open_browser: */ true, true)?;
    Ok(())
}

/// Race-free auto-start: a non-blocking exclusive flock on
/// `.sideview/spawn.lock`. Losing the lock means somebody else is already
/// spawning, which is success from our point of view.
fn spawn_detached(store: &Store, open_browser: bool, bind_auto: bool) -> Result<bool> {
    let lock = File::create(store.dir.join(SPAWN_LOCK))?;
    let got = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if !got {
        return Ok(false);
    }
    // Re-check under the lock: the daemon we raced may have claimed the row.
    if store.daemon_alive()?.map_or(false, |d| d.reachable) {
        return Ok(false);
    }
    let exe = std::env::current_exe()?;
    let log = File::create(store.dir.join("daemon.log"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("__daemon");
    if open_browser {
        cmd.arg("--open");
    }
    cmd.arg("--bind").arg(if bind_auto { "auto" } else { "loopback" });
    cmd.current_dir(&store.root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // Never parented to a tty.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().context("spawning daemon")?;
    // The lock releases when this process exits, slightly before the child
    // claims the row. A spawner racing into that window starts a second
    // daemon, and supersession heals it: the loser evicts itself.
    Ok(true)
}

/// Bare `sideview`: idempotent. A live reachable daemon means open the tab
/// and exit; otherwise start one (foreground by default) and open on ready.
pub fn open(detach: bool, bind: &str) -> Result<()> {
    let bind_auto = parse_bind(bind)?;
    let store = open_project_store()?;
    // Deliberately no session touch here: sessions exist when blocks do.
    // Minting one on bare open grew the switcher by an empty chip per shell.

    if let Some(d) = store.daemon_alive()? {
        if d.reachable {
            if d.version != env!("CARGO_PKG_VERSION") {
                eprintln!(
                    "daemon is running v{}, this is v{} — Ctrl-C it and run `sideview` again",
                    d.version,
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(1);
            }
            let url = format!("http://127.0.0.1:{}/", d.port);
            eprintln!("daemon already running");
            eprintln!("local:   {url}");
            open_browser(&url);
            return Ok(());
        }
        eprintln!(
            "recorded daemon is in an unreachable network namespace — claiming its row"
        );
    }

    let verdict = netcheck::verdict();
    if !verdict.reachable {
        bail!(
            "a daemon started here could never be reached by a browser ({}).\n\
             Run `sideview` in {} from outside the sandbox.",
            verdict.reasons.join(", "),
            store.root.display()
        );
    }

    if detach {
        if !spawn_detached(&store, false, bind_auto)? {
            eprintln!("another sideview is already starting the daemon");
        }
        // Readiness is the row appearing: wait briefly, honestly.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(d) = store.daemon_alive()? {
                if d.reachable {
                    let url = format!("http://127.0.0.1:{}/", d.port);
                    eprintln!("local:   {url}");
                    open_browser(&url);
                    return Ok(());
                }
            }
            if Instant::now() > deadline {
                bail!(
                    "daemon did not come up within 5s — check {}",
                    store.dir.join("daemon.log").display()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // Foreground: your terminal gets the log and Ctrl-C is the teardown.
    // Hold the spawn lock for the daemon's lifetime; it releases on exit.
    let lock = File::create(store.dir.join(SPAWN_LOCK))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another sideview is already starting a daemon here");
    }
    let dir = store.dir.clone();
    drop(store); // the daemon opens its own connections
    daemon::run(&dir, &daemon::Opts { bind_auto, open_browser: true })
}

/// `sideview session set` — the session-properties chunk: label and outline.
/// Resolves the session exactly like authoring, so an agent passes no ids.
pub fn session_set(
    explicit_session: Option<&str>,
    label: Option<&str>,
    outline: Option<&str>,
) -> Result<()> {
    if label.is_none() && outline.is_none() {
        bail!("nothing to set — pass --label and/or --outline");
    }
    if let Some(o) = outline {
        if o != "auto" && o != "off" {
            bail!("--outline takes `auto` or `off`, not {o:?}");
        }
    }
    let store = open_project_store()?;
    let session_id = resolve_and_touch(&store, explicit_session)?;
    if let Some(label) = label {
        // An empty label clears back to showing the session id.
        store.set_session_prop(&session_id, "label", (!label.is_empty()).then_some(label))?;
    }
    if let Some(outline) = outline {
        // auto is the default, so it stores as an absence.
        store.set_session_prop(&session_id, "outline", (outline != "auto").then_some(outline))?;
    }
    Ok(())
}

pub fn sessions() -> Result<()> {
    let store = open_project_store()?;
    let port = store.daemon_alive()?.filter(|d| d.reachable).map(|d| d.port);
    for s in store.sessions()? {
        let label = s.prop("label").unwrap_or(&s.id).to_string();
        match port {
            Some(p) => println!("{label}  http://127.0.0.1:{p}/s/{}", encode_session(&s.id)),
            None => println!("{label}  (no daemon running)"),
        }
    }
    Ok(())
}

pub fn status() -> Result<()> {
    let store = open_project_store()?;
    println!("store:   {}", store.db_path().display());
    match store.daemon()? {
        None => println!("daemon:  not running"),
        Some(d) => {
            // An explicit status runs the doubt path: definitive, not a
            // timestamp guess.
            let alive = store.ping_daemon()?;
            println!(
                "daemon:  {} — v{}, port {}, pid {}, reachable={}",
                if alive { "alive (answered ping)" } else { "NOT answering (stale row?)" },
                d.version,
                d.port,
                d.pid,
                d.reachable,
            );
            if d.version != env!("CARGO_PKG_VERSION") {
                println!(
                    "         version skew: this binary is v{} — Ctrl-C the daemon and run `sideview` again",
                    env!("CARGO_PKG_VERSION")
                );
            }
        }
    }
    println!("skill:   {}", crate::skill::status_line());
    Ok(())
}

pub fn styles() -> Result<()> {
    println!("{}", crate::skill::STYLES);
    Ok(())
}

/// Deleting the store under a running daemon is the nastiest failure
/// available here, and it is silent — so refuse while one is live.
pub fn reset() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let dir = store::find_store_dir(&cwd);
    if !dir.join(store::DB_FILE).exists() {
        eprintln!("nothing to reset at {}", dir.display());
        return Ok(());
    }
    let store = Store::open(&dir)?;
    if store.daemon_alive()?.is_some() {
        bail!("a daemon is running; Ctrl-C it first.");
    }
    drop(store);
    // The WAL and SHM go with it — a leftover WAL against a new database is
    // its own confusing failure.
    for name in [store::DB_FILE, "sideview.db-wal", "sideview.db-shm"] {
        let p = dir.join(name);
        if p.exists() {
            std::fs::remove_file(&p).with_context(|| format!("removing {}", p.display()))?;
        }
    }
    eprintln!("store deleted; next write starts fresh");
    Ok(())
}

/// Session ids go into printed URLs as one path segment, and the cwd and tmux
/// rungs produce ids containing `/` and `%`. Encode everything outside RFC
/// 3986's unreserved set — over-encoding is harmless, a raw slash is not.
fn encode_session(id: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SEGMENT: &AsciiSet =
        &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
    utf8_percent_encode(id, SEGMENT).to_string()
}

fn parse_bind(bind: &str) -> Result<bool> {
    match bind {
        "auto" => Ok(true),
        "loopback" => Ok(false),
        other => bail!("--bind takes `auto` or `loopback`, not {other:?}"),
    }
}

/// Try to open a browser; failure just means the printed URL is the path.
/// Inside an agent, don't even try — xdg-open needs a desktop session the
/// sandbox doesn't reach.
pub fn open_browser(url: &str) {
    if session::inside_agent() {
        return;
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

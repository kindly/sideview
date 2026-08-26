//! Everything the CLI does apart from being the daemon. Authoring commands
//! return immediately; only bare `sideview` ever waits, because only it needs
//! a URL to open a browser at.
//!
//! Since pages became files, authoring is file splicing: read the page's
//! `.sv` file under a lock, splice the block's line range, write atomically
//! (temp + rename, so the daemon never reads a torn file from *us* — agents
//! editing directly get the parser's tolerance instead). The store is only
//! touched to maintain the page→file binding.

use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::daemon;
use crate::format;
use crate::netcheck;
use crate::identity;
use crate::logic::base;
use crate::logic::edit::{append_block, edit_page, next_block_id, splice};
use crate::logic::conversation;
use crate::models::base::{self as store, Store, SPAWN_LOCK};

pub enum Kind {
    Prose,
    Markup,
    Html,
    Diff,
}

impl Kind {
    fn type_name(&self) -> &'static str {
        match self {
            Kind::Prose => "sv-prose",
            Kind::Markup => "sv-markup",
            Kind::Html => "sv-html",
            Kind::Diff => "sv-diff",
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

/// Resolve the page id, keep its binding fresh, and return the page file's
/// absolute path. The binding's path wins when one exists (a page may have
/// been re-bound after a move); otherwise it's the deterministic throwaway
/// location under `.sideview/pages/`.
fn resolve_and_bind(store: &Store, explicit: Option<&str>) -> Result<(String, PathBuf)> {
    let cwd = std::env::current_dir()?;
    let resolved = identity::resolve(explicit, &cwd);
    let rel = match base::binding(&store, &resolved.id)? {
        Some(b) => b.path,
        None => identity::page_rel_path(&resolved.id),
    };
    base::bind_page(&store, &resolved.id, &rel, &cwd.display().to_string(), resolved.detected_from)?;
    store.pages_dir()?; // ensure the directory exists before anyone writes into it
    Ok((resolved.id, store.root.join(rel)))
}

/// `sideview prose|markup|html|diff` — append the block to the page's
/// file, print its id alone on stdout, deal with the daemon afterwards, exit
/// without waiting. `extra` carries per-type attributes (html's --height).
pub fn author(kind: Kind, explicit_page: Option<&str>, extra: &[(&str, &str)]) -> Result<()> {
    for (k, v) in extra {
        format::check_attr_value(v).map_err(|e| anyhow::anyhow!("--{k}: {e}"))?;
    }
    let mut store = open_project_store()?;
    let (_, path) = resolve_and_bind(&store, explicit_page)?;
    let body = read_stdin()?;
    let mut assigned = String::new();
    edit_page(&path, |current| {
        let page = format::parse(&current);
        assigned = next_block_id(&page);
        let mut attrs: Vec<(&str, &str)> = vec![("id", &assigned)];
        attrs.extend_from_slice(extra);
        let block = format::block_text(kind.type_name(), &attrs, &body);
        Ok(append_block(current, &block))
    })?;
    println!("{assigned}");
    // Louder store identity (V4.sv's daily-driver bullet): two live
    // cross-project misfires argued this in — every write says where it
    // landed, so a resetting shell resolving the wrong store is visible on
    // the very next line instead of at debugging time.
    eprintln!("→ {}", store.root.display());
    ensure_daemon(&mut store)?;
    Ok(())
}

/// Replace a block's content (and possibly type) in place: splice its line
/// range, keep its other attributes. Short ids resolve within the caller's
/// own file, which is what makes two agents both holding a `b7` harmless.
pub fn update(short_id: &str, kind: Kind, explicit_page: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let (_, path) = resolve_and_bind(&store, explicit_page)?;
    let body = read_stdin()?;
    edit_page(&path, |current| {
        let page = format::parse(&current);
        let Some(b) = page.blocks.iter().find(|b| b.id() == Some(short_id)) else {
            bail!("no block {short_id} on this page");
        };
        let attrs: Vec<(&str, &str)> =
            b.attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let block = format::block_text(kind.type_name(), &attrs, &body);
        Ok(splice(&current, b.lines, Some(&block)))
    })?;
    eprintln!("→ {}", store.root.display());
    ensure_daemon(&mut store)?;
    Ok(())
}

/// Remove a block: its lines simply leave the file. No tombstone — clients
/// converge from the daemon's full-state connections (see daemon.rs).
pub fn rm(short_id: &str, explicit_page: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let (_, path) = resolve_and_bind(&store, explicit_page)?;
    edit_page(&path, |current| {
        let page = format::parse(&current);
        let Some(b) = page.blocks.iter().find(|b| b.id() == Some(short_id)) else {
            bail!("no block {short_id} on this page");
        };
        Ok(splice(&current, b.lines, None))
    })?;
    eprintln!("→ {}", store.root.display());
    ensure_daemon(&mut store)?;
    Ok(())
}

/// After a write: if a reachable daemon is alive, nothing to do (bar the
/// version-skew line). Otherwise auto-spawn unless this namespace is provably
/// unreachable, in which case print the one line that fixes it. The block is
/// already written either way — nothing is ever lost.
fn ensure_daemon(store: &mut Store) -> Result<()> {
    if let Some(d) = base::daemon_alive(&store)? {
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
    if base::daemon_alive(&store)?.map_or(false, |d| d.reachable) {
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
pub fn open(detach: bool, bind: &str, port: Option<u16>) -> Result<()> {
    let bind_auto = parse_bind(bind)?;
    let store = open_project_store()?;
    // Deliberately no page binding here: pages exist when blocks do.
    // Minting one on bare open grew the switcher by an empty chip per shell.

    if let Some(d) = base::daemon_alive(&store)? {
        if d.reachable {
            if d.version != env!("CARGO_PKG_VERSION") {
                eprintln!(
                    "daemon is running v{}, this is v{} — run `sideview restart`",
                    d.version,
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(1);
            }
            eprintln!("daemon already running");
            let url = print_urls(&store, d.port);
            daemon::open_browser(&url);
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
            if let Some(d) = base::daemon_alive(&store)? {
                if d.reachable {
                    let url = print_urls(&store, d.port);
                    daemon::open_browser(&url);
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
    daemon::run(&dir, &daemon::Opts { bind_auto, open_browser: true, port })
}

/// `sideview restart` — kill-first, then spawn (V4.sv's daily-driver
/// bullet). Kill-first because supersession cannot reuse a port the old
/// daemon still holds at bind time: only a full stop lets the new daemon
/// keep the remembered port, which is what lets every open tab's SSE simply
/// reconnect. The trigger is version skew after an upgrade; `status` and
/// bare `sideview` both name this command as the cure.
pub fn restart(bind: &str) -> Result<()> {
    let bind_auto = parse_bind(bind)?;
    let store = open_project_store()?;
    // Check reachability BEFORE killing anything: replacing a working
    // daemon with one no browser can reach is strictly worse than skew.
    let verdict = netcheck::verdict();
    if !verdict.reachable {
        bail!(
            "a daemon started here could never be reached by a browser ({}).\n\
             Run `sideview restart` in {} from outside the sandbox.",
            verdict.reasons.join(", "),
            store.root.display()
        );
    }

    if let Some(d) = base::daemon(&store)? {
        let old = format!("v{} (pid {})", d.version, d.pid);
        // ESRCH just means it's already gone — a stale row, not an error.
        unsafe { libc::kill(d.pid as libc::pid_t, libc::SIGTERM) };
        // Gone means: row cleared (graceful shutdown does this) AND the
        // port actually free — the whole reason this command exists.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let row_gone = base::daemon(&store)?.map_or(true, |now| now.pid != d.pid);
            let port_free =
                std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, d.port)).is_ok();
            if row_gone && port_free {
                break;
            }
            if Instant::now() > deadline {
                bail!(
                    "old daemon {old} has not released port {} after 10s — \
                     check it yourself, then run `sideview restart` again",
                    d.port
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("stopped {old}");
    } else {
        eprintln!("no daemon recorded — starting one");
    }

    if !spawn_detached(&store, false, bind_auto)? {
        eprintln!("another sideview is already starting the daemon");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(d) = base::daemon_alive(&store)? {
            if d.reachable {
                eprintln!("running v{} (pid {})", d.version, d.pid);
                print_urls(&store, d.port);
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

/// `sideview page set` — page properties now live in the file itself, on
/// the `<sv-page>` line: authored presentation belongs in the canonical
/// source, not in a database row.
pub fn page_set(
    explicit_page: Option<&str>,
    label: Option<&str>,
    outline: Option<&str>,
) -> Result<()> {
    if label.is_none() && outline.is_none() {
        bail!("nothing to set — pass --label and/or --outline");
    }
    if let Some(o) = outline {
        if !["auto", "scrollspy", "tabs", "off"].contains(&o) {
            bail!("--outline takes `auto`, `scrollspy`, `tabs` or `off`, not {o:?}");
        }
    }
    if let Some(l) = label {
        format::check_attr_value(l).map_err(|e| anyhow::anyhow!("--label: {e}"))?;
    }
    let store = open_project_store()?;
    let (_, path) = resolve_and_bind(&store, explicit_page)?;
    edit_page(&path, |current| {
        let page = format::parse(&current);
        let mut props: Vec<(String, String)> =
            page.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let set = |props: &mut Vec<(String, String)>, key: &str, value: Option<&str>| {
            props.retain(|(k, _)| k != key);
            if let Some(v) = value {
                props.push((key.to_string(), v.to_string()));
            }
        };
        if let Some(label) = label {
            // An empty label clears back to showing the page id.
            set(&mut props, "label", (!label.is_empty()).then_some(label));
        }
        if let Some(outline) = outline {
            // scrollspy is the default, so it (and `auto`) store as an
            // absence; only departures — tabs, off — are written.
            set(
                &mut props,
                "outline",
                (outline != "auto" && outline != "scrollspy").then_some(outline),
            );
        }
        let mut tag = String::from("<sv-page");
        for (k, v) in &props {
            tag.push_str(&format!(" {k}=\"{v}\""));
        }
        tag.push('>');
        // Rewrite the existing <sv-page> line, or open the file with one —
        // the closer is optional by grammar, so prepending is always safe.
        let lines: Vec<&str> = current.lines().collect();
        let page_line = lines.iter().position(|l| {
            let t = l.trim_end();
            t == "<sv-page>" || (t.starts_with("<sv-page ") && t.ends_with('>'))
        });
        let mut out: Vec<String> = Vec::new();
        match page_line {
            Some(i) => {
                out.extend(lines[..i].iter().map(|s| s.to_string()));
                out.push(tag);
                out.extend(lines[i + 1..].iter().map(|s| s.to_string()));
            }
            None => {
                out.push(tag);
                out.push(String::new());
                out.extend(lines.iter().map(|s| s.to_string()));
            }
        }
        Ok(out.join("\n") + "\n")
    })?;
    Ok(())
}

/// `sideview page rm [id]` — delete a page: its file, its sidecar lock,
/// its binding. No id means your own, matching `page set`. Deliberately
/// never touches the daemon: deletion must not auto-spawn one, and a running
/// one notices the binding vanish on its next tick.
pub fn page_rm(explicit_page: Option<&str>, id: Option<&str>, delete_file: bool) -> Result<()> {
    let mut store = open_project_store()?;
    let target = match id {
        Some(id) => id.to_string(),
        None => {
            let cwd = std::env::current_dir()?;
            identity::resolve(explicit_page, &cwd).id
        }
    };
    // The binding's path wins; without one, the deterministic throwaway
    // location — so `page rm` works even after a `reset` dropped the db.
    let rel = store
        .binding(&target)?
        .map(|b| b.path)
        .unwrap_or_else(|| identity::page_rel_path(&target));
    // Deletion splits by tier, not by format (author, 2026-08-10): a
    // throwaway page is sideview's own scratch and `rm` means rm; anything
    // in the repo — a promoted .sv as much as an imported DESIGN.md — is a
    // committed file, so the default is to unbind and leave it on disk.
    // `--file` is the deliberate second word for actually deleting one.
    if !store::is_throwaway_page(&rel) && !delete_file {
        let had = base::delete_binding(&mut store, &target)?;
        if !had {
            bail!("no page {target}");
        }
        eprintln!("unbound page {target} — {rel} is a committed file and stays on disk");
        eprintln!("(delete it with `sideview page rm {target} --file`, or with git)");
        if crate::config::load(&store.root).0.pages.iter().any(|e| e.page_id() == target) {
            eprintln!("note: .sideview.toml still lists it, so it will re-bind — remove the entry to keep it closed");
        }
        return Ok(());
    }
    let file = store.root.join(&rel);
    let had_binding = base::delete_binding(&mut store, &target)?;
    let had_file = match std::fs::remove_file(&file) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e).with_context(|| format!("removing {}", file.display())),
    };
    let _ = std::fs::remove_file(file.with_extension("sv.lock"));
    if !had_binding && !had_file {
        bail!("no page {target}");
    }
    eprintln!("removed page {target} ({rel}) → {}", store.root.display());
    Ok(())
}

/// `sideview open <file>` — bind a committed page. The binding id is the
/// file stem (V2.sv → "V2"): stable, guessable, and recognizable in the URL.
/// This is the missing verb from V2.sv's document-pages section; before it,
/// committed pages needed a hand-written INSERT.
pub fn open_page(file: &Path) -> Result<()> {
    let mut store = open_project_store()?;
    let cwd = std::env::current_dir()?;
    let abs = cwd.join(file);
    let abs = abs
        .canonicalize()
        .with_context(|| format!("no file {}", abs.display()))?;
    let root = store.root.canonicalize()?;
    let rel = abs
        .strip_prefix(&root)
        .map_err(|_| anyhow::anyhow!("{} is outside the project ({})", abs.display(), root.display()))?
        .to_string_lossy()
        .to_string();
    let id = abs
        .file_stem()
        .and_then(|s| s.to_str())
        .context("file has no stem to name the page after")?
        .to_string();
    base::bind_page(&store, &id, &rel, &cwd.display().to_string(), "open")?;
    eprintln!("bound page {id} → {rel}");
    ensure_daemon(&mut store)?;
    if let Some(d) = base::daemon_alive(&store)?.filter(|d| d.reachable) {
        eprintln!("http://127.0.0.1:{}/p/{}", d.port, identity::encode(&id));
    }
    Ok(())
}

/// `sideview page promote <dest>` — mv a throwaway page into the repo with
/// the binding following. The file is already canon-shaped; promotion just
/// gives it a version-control-worthy address.
pub fn page_promote(explicit_page: Option<&str>, dest: &Path) -> Result<()> {
    if dest.is_absolute() || dest.components().any(|c| c.as_os_str() == "..") {
        bail!("destination must be a relative path inside the project");
    }
    let store = open_project_store()?;
    let cwd = std::env::current_dir()?;
    let id = identity::resolve(explicit_page, &cwd).id;
    let Some(binding) = base::binding(&store, &id)? else {
        bail!("no page bound for {id}");
    };
    let from = store.root.join(&binding.path);
    let to = store.root.join(dest);
    if to.exists() {
        bail!("{} already exists", to.display());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&from, &to)
        .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
    let _ = std::fs::remove_file(from.with_extension("sv.lock"));
    base::rebind_path(&store, &id, &dest.to_string_lossy())?;
    eprintln!("promoted {} → {} (binding follows)", binding.path, dest.display());
    Ok(())
}

/// `sideview comment` — the db half of annotations, and the agent's way to
/// speak in a page's conversation. An anchor form creates a fresh thread
/// (threads succeed each other at an anchor); `--thread` replies. Prints the
/// thread id — what `resolve` and further replies address.
pub fn comment(
    block: Option<&str>,
    at: Option<&str>,
    thread: Option<i64>,
    page: Option<&str>,
) -> Result<()> {
    let mut store = open_project_store()?;
    let body = read_stdin()?;
    let body = body.trim_end_matches('\n');
    if body.is_empty() {
        bail!("empty comment (body arrives on stdin)");
    }
    let page_owned;
    let target = match (thread, block) {
        (Some(_), Some(_)) => {
            bail!("pass a block to start a thread, or --thread to reply — not both")
        }
        (Some(tid), None) => {
            if at.is_some() {
                bail!("--at places a new thread; a reply inherits its thread's anchor");
            }
            conversation::CommentTarget::Reply { thread: tid }
        }
        (None, Some(target)) => {
            // Commenting never mints a page binding — resolve without binding.
            let cwd = std::env::current_dir()?;
            let page_id = match page {
                Some(p) => p.to_string(),
                None => identity::resolve(None, &cwd).id,
            };
            page_owned = page_id;
            conversation::CommentTarget::NewThread {
                page: &page_owned,
                target,
                anchor: at.unwrap_or(""),
                quote: None,
                context: None,
            }
        }
        (None, None) => bail!("name a block to comment on, or --thread to reply"),
    };
    let (thread_id, _) =
        conversation::post_comment(&mut store, target, body, Some("agent"), None, "comment", &[], page)?;
    println!("{thread_id}");
    eprintln!("→ {}", store.root.display());
    Ok(())
}

/// `sideview resolve <thread>` — the agent's "feedback addressed"; `--undo`
/// reopens. Never a delete: the thread keeps its conversation and its place
/// in the page-tail list.
pub fn resolve(thread: i64, undo: bool, page: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let Some((t, changed)) = conversation::resolve(&mut store, thread, Some("agent"), undo, page)? else {
        bail!("no thread {thread}");
    };
    if !changed {
        // The state it's already in, said plainly — not an error worth a
        // nonzero exit, since the desired end state holds.
        eprintln!(
            "thread {thread} is already {}",
            if undo { "open" } else { "resolved" }
        );
        return Ok(());
    }
    eprintln!(
        "{} thread {thread} on {} ({})",
        if undo { "reopened" } else { "resolved" },
        t.page,
        t.target
    );
    Ok(())
}

/// `sideview working <thread>` — the agent's "this will take a while".
/// Between the plumbing's sent receipt and the eventual reply, the bar shows
/// working; the reply (or a resolve) retires it automatically.
pub fn working(thread: i64, page: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let Some((_, changed)) = conversation::working(&mut store, thread, Some("agent"), page)? else {
        bail!("no thread {thread}");
    };
    if !changed {
        eprintln!("thread {thread} is resolved — nothing to work on");
        return Ok(());
    }
    eprintln!("working on thread {thread} → {}", store.root.display());
    Ok(())
}

/// `sideview outline` — the agent's ordered list for the rail, used verbatim
/// when present (inference off). Entries arrive as JSON on stdin:
/// [{"title": "Overview", "anchor": "h:b2-overview", "children": […]}].
pub fn outline(clear: bool, page: Option<&str>) -> Result<()> {
    let mut store = open_project_store()?;
    let cwd = std::env::current_dir()?;
    let page_id = match page {
        Some(p) => p.to_string(),
        None => identity::resolve(None, &cwd).id,
    };
    if clear {
        if !base::clear_outline(&mut store, &page_id)? {
            eprintln!("no explicit outline on {page_id}");
        }
        return Ok(());
    }
    let spec = read_stdin()?;
    // Validate the shape now — a broken outline should fail the command, not
    // quietly wedge the rail.
    let parsed: serde_json::Value =
        serde_json::from_str(&spec).context("outline entries must be JSON")?;
    if !parsed.is_array() {
        bail!("outline entries are a JSON array of {{title, anchor, children?}}");
    }
    base::set_outline(&mut store, &page_id, &parsed.to_string())?;
    Ok(())
}

/// `sideview watch` — the agent's await: a blocking read on the store
/// itself. Typed JSON-lines on stdout (comment / resolve / unresolve), one
/// object per line. Sandbox-compatible (SQLite file access, no network) and
/// daemon-independent. One delivery concept: every watcher sees everything
/// its filter keeps; `--ack` receipts what it emits. `--page` and
/// `--category` (both repeatable) scope the watch — the decisions live in
/// conversation::watch_tick, this loop only paces and prints.
pub fn watch(
    timeout: Option<u64>,
    since: Option<i64>,
    pages: Vec<String>,
    categories: Vec<String>,
    skip_author: Option<&str>,
    ack: bool,
) -> Result<()> {
    use std::io::Write as _;

    let mut store = open_project_store()?;
    let whoami = format!("watch:{}", std::process::id());
    let deadline = timeout.map(|t| Instant::now() + Duration::from_secs(t));

    let mut state = conversation::watch_start(&store, since)?;
    let filter = conversation::WatchFilter {
        skip_author: skip_author.map(str::to_string),
        pages,
        categories,
    };
    let ack_by = ack.then_some(whoami.as_str());

    let mut out = std::io::stdout();
    loop {
        for line in conversation::watch_tick(&mut store, &mut state, &filter, ack_by)? {
            writeln!(out, "{line}")?;
            out.flush()?;
        }
        if deadline.map_or(false, |d| Instant::now() >= d) {
            return Ok(()); // --timeout gives up quietly
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// `sideview attachments gc` — the deliberate cleanup verb (V3.sv's
/// lifecycle law; a startup auto-sweep was considered and dropped, because
/// after the resurrection test it would silently take everything). Default:
/// delete only files referenced by no row *and* no current page's content.
/// --resolved widens to files held only by resolved threads. Its writ never
/// leaves `.sideview/attachments/`.
pub fn attachments_gc(resolved: bool) -> Result<()> {
    let mut store = open_project_store()?;
    let home = store.dir.join(conversation::ATTACHMENTS_DIR);

    let mut on_disk: Vec<String> = Vec::new();
    let mut stack = vec![home.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(&store.root) {
                on_disk.push(rel.to_string_lossy().to_string());
            }
        }
    }
    if on_disk.is_empty() {
        println!("nothing in {}", home.display());
        return Ok(());
    }
    on_disk.sort();

    let refs: std::collections::HashMap<String, bool> =
        conversation::attachment_refs(&store)?.into_iter().collect();
    // Every current page's raw content — the backstop that protects a file a
    // page references directly, and names it as mis-homed while doing so.
    let pages: Vec<(String, String)> = store
        .bindings()?
        .into_iter()
        .filter_map(|b| std::fs::read_to_string(store.root.join(&b.path)).ok().map(|s| (b.id, s)))
        .collect();

    let (mut collected, mut freed) = (0u64, 0u64);
    let (mut open_held, mut resolved_held) = (0u64, 0u64);
    for rel in on_disk {
        let in_page = pages.iter().find(|(_, src)| src.contains(&rel)).map(|(id, _)| id.clone());
        let take = match (refs.get(&rel), &in_page) {
            (_, Some(page)) => {
                println!("kept: {rel} — referenced by page {page:?} but owned by conversation; promote it into the page's own home");
                false
            }
            (Some(true), None) => {
                open_held += 1;
                false
            }
            (Some(false), None) => {
                if resolved {
                    conversation::delete_attachment_rows(&mut store, &rel)?;
                    true
                } else {
                    resolved_held += 1;
                    false
                }
            }
            (None, None) => true, // nothing anywhere: a canceled draft, or conversation that died
        };
        if take {
            freed += std::fs::metadata(store.root.join(&rel)).map(|m| m.len()).unwrap_or(0);
            conversation::unlink_attachment(&store, &rel);
            collected += 1;
            println!("collected {rel}");
        }
    }
    println!(
        "{collected} file(s) collected ({} KB freed); {open_held} held by open threads; {resolved_held} held only by resolved threads{}",
        freed / 1024,
        if resolved_held > 0 { " (widen with --resolved)" } else { "" }
    );
    Ok(())
}

pub fn pages() -> Result<()> {
    let store = open_project_store()?;
    let port = base::daemon_alive(&store)?.filter(|d| d.reachable).map(|d| d.port);
    let cfg = crate::config::load(&store.root).0;
    for b in base::bindings(&store)? {
        // Only a composed page carries an sv-page tag. Parsing an imported
        // one as sv is not merely pointless but wrong: V1.md documents the
        // format, so its *example* tag was read as a real one (found by
        // dogfooding, 2026-08-10). Imported labels come from config.
        let label = if crate::config::is_imported(&b.path) {
            cfg.pages
                .iter()
                .find(|e| e.path == b.path)
                .and_then(|e| e.label.clone())
                .unwrap_or_else(|| b.id.clone())
        } else {
            std::fs::read_to_string(store.root.join(&b.path))
                .ok()
                .and_then(|src| format::parse(&src).prop("label").map(str::to_string))
                .unwrap_or_else(|| b.id.clone())
        };
        match port {
            Some(p) => println!(
                "{label}  {}  http://127.0.0.1:{p}/p/{}",
                b.path,
                identity::encode(&b.id)
            ),
            None => println!("{label}  {}  (no daemon running)", b.path),
        }
    }
    Ok(())
}

pub fn status() -> Result<()> {
    let store = open_project_store()?;
    println!("store:   {}", store.db_path().display());
    println!("pages:   {}", store.dir.join(store::PAGES_DIR).display());
    match base::daemon(&store)? {
        None => println!("daemon:  not running"),
        Some(d) => {
            // An explicit status runs the doubt path: definitive, not a
            // timestamp guess.
            let alive = base::ping_daemon(&store)?;
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
                    "         version skew: this binary is v{} — run `sideview restart`",
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
/// available here, and it is silent — so refuse while one is live. Pages are
/// content, not store: reset never touches them.
pub fn reset() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let dir = store::find_store_dir(&cwd);
    if !dir.join(store::DB_FILE).exists() {
        eprintln!("nothing to reset at {}", dir.display());
        return Ok(());
    }
    let store = Store::open(&dir)?;
    if base::daemon_alive(&store)?.is_some() {
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
    eprintln!(
        "store deleted; pages under {} kept. Next write rebinds them.",
        dir.join(store::PAGES_DIR).display()
    );
    Ok(())
}

fn parse_bind(bind: &str) -> Result<bool> {
    match bind {
        "auto" => Ok(true),
        "loopback" => Ok(false),
        other => bail!("--bind takes `auto` or `loopback`, not {other:?}"),
    }
}

/// Print every URL the daemon bound (recorded in meta at claim time), and
/// return the local one for the browser. Falls back to loopback+port when
/// the meta is missing — an older daemon, or a row left by a crash.
fn print_urls(store: &Store, port: u16) -> String {
    let urls: Vec<String> = store
        .meta("urls")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let urls = if urls.is_empty() { vec![format!("http://127.0.0.1:{port}")] } else { urls };
    for (i, url) in urls.iter().enumerate() {
        if i == 0 {
            eprintln!("local:   {url}");
        } else {
            eprintln!("tailnet: {url}      (any tailnet node can read this)");
        }
    }
    format!("{}/", urls[0])
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_assigns_ids_past_the_highest_ever_used() {
        let v1 = append_block(String::new(), &format::block_text("sv-prose", &[("id", "b1")], "one"));
        let page = format::parse(&v1);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(next_block_id(&page), "b2");
        // rm b1, then the next id must still be b2 — ids never recycle…
        let b1 = page.blocks[0].lines;
        let v2 = splice(&v1, b1, None);
        // …except that an empty file has no memory; the file *is* the state,
        // so a fully emptied page restarts at b1. Within a live page, the max
        // survives because blocks rarely all vanish at once.
        let after = format::parse(&v2);
        assert_eq!(after.blocks.len(), 0);
        assert!(format::parse(&v2).prop("label").is_none());
    }

    #[test]
    fn append_lands_inside_the_page_wrapper() {
        let one = append_block(String::new(), &format::block_text("sv-prose", &[("id", "b1")], "one"));
        let two = append_block(one, &format::block_text("sv-markup", &[("id", "b2")], "<b>two</b>"));
        let page = format::parse(&two);
        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[1].id(), Some("b2"));
        assert!(
            two.trim_end().ends_with("</sv-page>"),
            "blocks go inside the wrapper: {two}"
        );
    }

    #[test]
    fn splice_replaces_a_block_in_place() {
        let src = "<sv-page>\n\n<sv-prose id=\"b1\">\none\n</sv-prose>\n\n<sv-prose id=\"b2\">\ntwo\n</sv-prose>\n\n</sv-page>\n";
        let page = format::parse(src);
        let replaced = splice(
            src,
            page.blocks[0].lines,
            Some(&format::block_text("sv-markup", &[("id", "b1")], "<i>uno</i>")),
        );
        let after = format::parse(&replaced);
        assert_eq!(after.blocks.len(), 2);
        assert_eq!(after.blocks[0].type_name, "sv-markup");
        assert_eq!(after.blocks[0].body, "<i>uno</i>");
        assert_eq!(after.blocks[1].body, "two", "the neighbour is untouched");
    }

    #[test]
    fn splice_removal_does_not_stack_blank_lines() {
        let src = "<sv-page>\n\n<sv-prose id=\"b1\">\none\n</sv-prose>\n\n<sv-prose id=\"b2\">\ntwo\n</sv-prose>\n\n</sv-page>\n";
        let page = format::parse(src);
        let removed = splice(src, page.blocks[0].lines, None);
        assert!(!removed.contains("\n\n\n"), "no doubled blanks: {removed:?}");
        assert_eq!(format::parse(&removed).blocks.len(), 1);
    }
}

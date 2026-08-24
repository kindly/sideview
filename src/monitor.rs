//! Harness-specific feedback monitors. `watch` stays a pure JSON-lines
//! reader; this module owns the side-effectful Codex queue lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::logic::conversation::{self as conversation, Comment, Thread};
use crate::models::base::{self as store, Store};

const STATE_FILE: &str = "monitor-codex.json";
const LOCK_FILE: &str = "monitor-codex.lock";
const SPAWN_LOCK_FILE: &str = "monitor-codex-spawn.lock";
const LOG_FILE: &str = "monitor-codex.log";
const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResolutionState {
    resolved_at: Option<i64>,
    resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MonitorState {
    version: u32,
    instance_id: String,
    pid: u32,
    pid_start: Option<u64>,
    started_at: i64,
    stopped_at: Option<i64>,
    detached: bool,
    status: String,
    thread: String,
    codex_path: String,
    cursor: i64,
    resolutions: BTreeMap<i64, ResolutionState>,
    pending_event: Option<String>,
    last_event_at: Option<i64>,
    last_queued_at: Option<i64>,
    last_queue_output: Option<String>,
    last_error: Option<String>,
}

fn open_project_store() -> Result<Store> {
    let cwd = std::env::current_dir()?;
    Store::open(&store::find_store_dir(&cwd))
}

fn state_path(store: &Store) -> PathBuf {
    store.dir.join(STATE_FILE)
}

fn lock_path(store: &Store) -> PathBuf {
    store.dir.join(LOCK_FILE)
}

fn load_state(store: &Store) -> Result<Option<MonitorState>> {
    match std::fs::read_to_string(state_path(store)) {
        Ok(src) => Ok(Some(
            serde_json::from_str(&src).context("reading Codex monitor state")?,
        )),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading Codex monitor state"),
    }
}

fn write_state(store: &Store, state: &MonitorState) -> Result<()> {
    let path = state_path(store);
    let tmp = store
        .dir
        .join(format!("{STATE_FILE}.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(state)?;
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn try_lock(path: &Path) -> Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(Some(file));
    }
    let err = std::io::Error::last_os_error();
    let code = err.raw_os_error();
    if code == Some(libc::EWOULDBLOCK) || code == Some(libc::EAGAIN) {
        return Ok(None);
    }
    Err(err).with_context(|| format!("locking {}", path.display()))
}

fn lock_is_held(path: &Path) -> Result<bool> {
    match try_lock(path)? {
        Some(file) => {
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
            Ok(false)
        }
        None => Ok(true),
    }
}

fn resolve_thread(explicit: Option<&str>) -> Result<String> {
    let value = explicit
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("CODEX_THREAD_ID")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .context(
            "no Codex thread id — run from Codex (where CODEX_THREAD_ID is available) \
             or pass `--thread <uuid>`; the monitor never guesses the latest thread",
        )?;
    let parsed = uuid::Uuid::parse_str(value.trim())
        .with_context(|| format!("Codex thread id {value:?} is not a UUID"))?;
    Ok(parsed.to_string())
}

fn path_candidates(explicit: Option<&Path>) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Some(path) = explicit {
        out.push(("--codex-path".into(), path.to_path_buf()));
        return out;
    }
    if let Some(path) = std::env::var_os("SIDEVIEW_CODEX_PATH").filter(|s| !s.is_empty()) {
        out.push(("SIDEVIEW_CODEX_PATH".into(), PathBuf::from(path)));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            out.push((
                "PATH".into(),
                dir.join(if cfg!(windows) { "codex.exe" } else { "codex" }),
            ));
        }
    }
    #[cfg(target_os = "linux")]
    out.push((
        "ChatGPT desktop bundle".into(),
        "/usr/lib/chatgpt/resources/codex".into(),
    ));
    #[cfg(target_os = "macos")]
    {
        out.push((
            "ChatGPT desktop bundle".into(),
            "/Applications/ChatGPT.app/Contents/Resources/codex".into(),
        ));
        out.push((
            "Codex desktop bundle".into(),
            "/Applications/Codex.app/Contents/Resources/codex".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    out.retain(|(_, p)| seen.insert(p.clone()));
    out
}

fn preflight_codex(path: &Path) -> Result<()> {
    let out = Command::new(path)
        .args(["queue", "--help"])
        .output()
        .with_context(|| format!("running {} queue --help", path.display()))?;
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() || !help.contains("--thread") || !help.contains("--message") {
        bail!("does not expose a compatible `queue --thread … --message …` command");
    }
    Ok(())
}

fn discover_codex(explicit: Option<&Path>) -> Result<PathBuf> {
    let candidates = path_candidates(explicit);
    let mut checked = Vec::new();
    for (source, path) in candidates {
        if !path.is_file() {
            checked.push(format!("- {source}: {} (not found)", path.display()));
            continue;
        }
        match preflight_codex(&path) {
            Ok(()) => return Ok(std::fs::canonicalize(&path).unwrap_or(path)),
            Err(e) => checked.push(format!("- {source}: {} ({e})", path.display())),
        }
    }
    bail!(
        "could not find a compatible Codex CLI binary. Checked:\n{}\n\
         Point Sideview at one with `--codex-path <path>` or SIDEVIEW_CODEX_PATH, \
         or install the official Codex CLI; Sideview will not install it silently.",
        if checked.is_empty() {
            "- no candidates".into()
        } else {
            checked.join("\n")
        }
    )
}

fn resolution_snapshot(store: &Store) -> Result<BTreeMap<i64, ResolutionState>> {
    Ok(conversation::thread_resolutions(store)?
        .into_iter()
        .map(|(id, _, resolved_at, resolved_by)| {
            (
                id,
                ResolutionState {
                    resolved_at,
                    resolved_by,
                },
            )
        })
        .collect())
}

fn comment_message(comment: &Comment, thread: &Thread, attachment_count: usize) -> String {
    let label = match comment.kind.as_str() {
        "edit" => "Sideview edit request",
        "edited" => "Sideview block edit notice",
        _ => "Sideview comment",
    };
    let mut parts = vec![
        label.to_string(),
        format!(
            "Page {} · thread {} · event {}",
            thread.page, thread.id, comment.id
        ),
    ];
    if let Some(quote) = thread.quote.as_deref().filter(|q| !q.is_empty()) {
        parts.push(format!("Quoted text: “{quote}”"));
    }
    if !comment.body.is_empty() {
        parts.push(format!("Feedback:\n{}", comment.body));
    }
    if attachment_count > 0 {
        parts.push(format!(
            "Attachments: {attachment_count} (inspect the Sideview event/thread before reading any bytes)"
        ));
    }
    parts.push(
        "Respond in context. For a comment, reply on the named Sideview thread with the page guard and do not resolve it. For an edit request, follow the Sideview edit workflow."
            .into(),
    );
    parts.join("\n\n")
}

fn resolution_message(id: i64, page: &str, current: &ResolutionState) -> String {
    if current.resolved_at.is_some() {
        format!(
            "Sideview thread resolved by the user\n\nPage {page} · thread {id}\n\n\
             Leave it resolved; no Sideview reply is needed. Keep monitoring."
        )
    } else {
        format!(
            "Sideview thread reopened by the user\n\nPage {page} · thread {id}\n\n\
             Review the thread in context and respond if action is needed."
        )
    }
}

fn queue_message(codex: &Path, thread: &str, message: &str) -> Result<String> {
    let out = Command::new(codex)
        .args(["queue", "--thread", thread, "--message", message])
        .output()
        .with_context(|| format!("running {} queue", codex.display()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        bail!(
            "codex queue exited {}: {}{}",
            out.status,
            stderr,
            if stdout.is_empty() {
                String::new()
            } else {
                format!(" ({stdout})")
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn deliver(store: &Store, state: &mut MonitorState, key: &str, message: &str) -> Result<()> {
    state.pending_event = Some(key.to_string());
    state.last_event_at = Some(store::now_ms());
    state.last_error = None;
    write_state(store, state)?;
    match queue_message(Path::new(&state.codex_path), &state.thread, message) {
        Ok(output) => {
            state.pending_event = None;
            state.last_queued_at = Some(store::now_ms());
            state.last_queue_output = (!output.is_empty()).then_some(output);
            state.last_error = None;
            Ok(())
        }
        Err(e) => {
            state.last_error = Some(format!("{e:#}"));
            write_state(store, state)?;
            Err(e)
        }
    }
}

fn process_generation(store: &mut Store, state: &mut MonitorState) -> Result<()> {
    for (comment, thread) in conversation::comments_after(store, state.cursor)? {
        if comment.author.as_deref() == Some("agent") {
            state.cursor = comment.id;
            write_state(store, state)?;
            continue;
        }
        let attachments = conversation::attachments_for_comment(store, comment.id)?;
        let message = comment_message(&comment, &thread, attachments.len());
        deliver(store, state, &format!("comment:{}", comment.id), &message)?;
        // External delivery succeeded. Persist the cursor before the optional
        // page receipt: a receipt failure must not duplicate the model turn.
        state.cursor = comment.id;
        write_state(store, state)?;
        if let Err(e) = conversation::ack_comment(store, comment.id, "monitor:codex") {
            eprintln!(
                "queued comment {} but could not stamp its receipt: {e:#}",
                comment.id
            );
        }
    }

    let rows = conversation::thread_resolutions(store)?;
    let present: BTreeSet<i64> = rows.iter().map(|(id, _, _, _)| *id).collect();
    for (id, page, resolved_at, resolved_by) in rows {
        let current = ResolutionState {
            resolved_at,
            resolved_by,
        };
        let Some(previous) = state.resolutions.get(&id).cloned() else {
            // A new thread announces itself through its first comment.
            state.resolutions.insert(id, current);
            continue;
        };
        if previous == current {
            continue;
        }
        if current.resolved_by.as_deref() == Some("agent") {
            state.resolutions.insert(id, current);
            continue;
        }
        let message = resolution_message(id, &page, &current);
        let kind = if current.resolved_at.is_some() {
            "resolve"
        } else {
            "unresolve"
        };
        deliver(store, state, &format!("{kind}:{id}"), &message)?;
        state.resolutions.insert(id, current);
        write_state(store, state)?;
    }
    state.resolutions.retain(|id, _| present.contains(id));
    write_state(store, state)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn pid_start(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = stat.rsplit_once(')')?.1.trim();
    // tail starts at field 3 (state); starttime is field 22.
    tail.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn pid_start(_pid: u32) -> Option<u64> {
    None
}

fn pid_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn pid_matches(state: &MonitorState) -> bool {
    if !pid_alive(state.pid) {
        return false;
    }
    match state.pid_start {
        Some(expected) => pid_start(state.pid) == Some(expected),
        None => true,
    }
}

fn run_monitor(thread: String, codex: PathBuf, detached: bool, instance_id: String) -> Result<()> {
    let mut store = open_project_store()?;
    let _lock = try_lock(&lock_path(&store))?.context(
        "a Codex monitor is already running for this project; use `sideview monitor status`",
    )?;
    let previous = load_state(&store)?;
    let current_max = conversation::max_comment_id(&store)?;
    let reuse = previous
        .as_ref()
        .filter(|s| s.version == STATE_VERSION && s.thread == thread && s.cursor <= current_max);
    let cursor = reuse.map_or(current_max, |s| s.cursor);
    let resolutions = reuse
        .map(|s| s.resolutions.clone())
        .unwrap_or(resolution_snapshot(&store)?);
    let mut state = MonitorState {
        version: STATE_VERSION,
        instance_id,
        pid: std::process::id(),
        pid_start: pid_start(std::process::id()),
        started_at: store::now_ms(),
        stopped_at: None,
        detached,
        status: "running".into(),
        thread,
        codex_path: codex.display().to_string(),
        cursor,
        resolutions,
        pending_event: reuse.and_then(|s| s.pending_event.clone()),
        last_event_at: reuse.and_then(|s| s.last_event_at),
        last_queued_at: reuse.and_then(|s| s.last_queued_at),
        last_queue_output: reuse.and_then(|s| s.last_queue_output.clone()),
        last_error: None,
    };
    write_state(&store, &state)?;
    eprintln!(
        "Codex monitor running for thread {} (pid {}, cursor {})",
        state.thread, state.pid, state.cursor
    );

    let mut generation = -1i64;
    let mut retry = Duration::from_secs(1);
    loop {
        let current = match conversation::generation(&store) {
            Ok(g) => g,
            Err(e) => {
                state.last_error = Some(format!("reading Sideview store: {e:#}"));
                let _ = write_state(&store, &state);
                eprintln!(
                    "monitor store error: {e:#}; retrying in {}s",
                    retry.as_secs()
                );
                std::thread::sleep(retry);
                retry = (retry * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        if current != generation {
            match process_generation(&mut store, &mut state) {
                Ok(()) => {
                    generation = current;
                    retry = Duration::from_secs(1);
                }
                Err(e) => {
                    eprintln!(
                        "monitor delivery error: {e:#}; retrying in {}s",
                        retry.as_secs()
                    );
                    std::thread::sleep(retry);
                    retry = (retry * 2).min(Duration::from_secs(30));
                    continue;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn start(
    explicit_thread: Option<&str>,
    explicit_codex: Option<&Path>,
    detach: bool,
) -> Result<()> {
    let thread = resolve_thread(explicit_thread)?;
    let codex = discover_codex(explicit_codex)?;
    let store = open_project_store()?;
    let spawn_lock = try_lock(&store.dir.join(SPAWN_LOCK_FILE))?
        .context("another process is starting or stopping a Codex monitor")?;
    if lock_is_held(&lock_path(&store))? {
        bail!("a Codex monitor is already running; use `sideview monitor status`");
    }
    let instance = uuid::Uuid::new_v4().to_string();
    if !detach {
        drop(store);
        drop(spawn_lock);
        return run_monitor(thread, codex, false, instance);
    }

    if lock_is_held(&lock_path(&store))? {
        bail!("a Codex monitor is already running; use `sideview monitor status`");
    }
    let log = File::create(store.dir.join(LOG_FILE))?;
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("__monitor-codex")
        .arg("--thread")
        .arg(&thread)
        .arg("--codex-path")
        .arg(&codex)
        .arg("--instance")
        .arg(&instance)
        .current_dir(&store.root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().context("spawning detached Codex monitor")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(state) = load_state(&store)? {
            if state.instance_id == instance && lock_is_held(&lock_path(&store))? {
                println!(
                    "Codex monitor running for thread {} (pid {}, cursor {})",
                    state.thread, state.pid, state.cursor
                );
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            bail!(
                "Codex monitor did not start within 5s — check {}",
                store.dir.join(LOG_FILE).display()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn run_internal(thread: String, codex: PathBuf, instance: String) -> Result<()> {
    run_monitor(thread, codex, true, instance)
}

pub fn status() -> Result<()> {
    let store = open_project_store()?;
    let Some(state) = load_state(&store)? else {
        println!("Codex monitor: not configured");
        return Ok(());
    };
    let held = lock_is_held(&lock_path(&store))?;
    let running = held && pid_matches(&state);
    println!(
        "Codex monitor: {}",
        if running {
            "running"
        } else if held {
            "ownership mismatch"
        } else {
            "stopped"
        }
    );
    println!("thread:  {}", state.thread);
    println!("pid:     {}", state.pid);
    println!("codex:   {}", state.codex_path);
    println!("cursor:  {}", state.cursor);
    if let Some(pending) = state.pending_event.as_deref() {
        println!("pending: {pending}");
    }
    if let Some(error) = state.last_error.as_deref() {
        println!("error:   {error}");
    }
    Ok(())
}

pub fn stop() -> Result<()> {
    let store = open_project_store()?;
    let _spawn_lock = try_lock(&store.dir.join(SPAWN_LOCK_FILE))?
        .context("another process is starting or stopping a Codex monitor")?;
    let Some(mut state) = load_state(&store)? else {
        println!("Codex monitor is not running");
        return Ok(());
    };
    if !lock_is_held(&lock_path(&store))? {
        println!(
            "Codex monitor is not running (stale state for pid {})",
            state.pid
        );
        return Ok(());
    }
    if !pid_matches(&state) {
        bail!(
            "refusing to stop pid {}: the recorded process identity does not match the running process",
            state.pid
        );
    }
    unsafe { libc::kill(state.pid as libc::pid_t, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(5);
    while lock_is_held(&lock_path(&store))? {
        if Instant::now() >= deadline {
            bail!("Codex monitor pid {} did not stop within 5s", state.pid);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    state.status = "stopped".into();
    state.stopped_at = Some(store::now_ms());
    state.pending_event = None;
    write_state(&store, &state)?;
    println!("stopped Codex monitor pid {}", state.pid);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn test_store() -> Store {
        let root = std::env::temp_dir().join(format!("sideview-monitor-{}", uuid::Uuid::new_v4()));
        Store::open(&root.join(store::DIR_NAME)).unwrap()
    }

    fn fake_codex(store: &Store, succeeds: bool) -> (PathBuf, PathBuf) {
        let bin = store.root.join("fake-codex");
        let calls = store.root.join("queue-calls.txt");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = queue ] && [ \"$2\" = --help ]; then\n  echo 'queue --thread ID --message TEXT'\n  exit 0\nfi\nprintf '%s\\n' \"$@\" >> '{}'\n{}\n",
            calls.display(),
            if succeeds {
                "echo 'Queued message fake-id'\nexit 0"
            } else {
                "echo 'queue refused' >&2\nexit 2"
            }
        );
        std::fs::write(&bin, script).unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).unwrap();
        (bin, calls)
    }

    fn state(codex: &Path, cursor: i64) -> MonitorState {
        MonitorState {
            version: STATE_VERSION,
            instance_id: uuid::Uuid::new_v4().to_string(),
            pid: std::process::id(),
            pid_start: pid_start(std::process::id()),
            started_at: store::now_ms(),
            stopped_at: None,
            detached: false,
            status: "running".into(),
            thread: uuid::Uuid::new_v4().to_string(),
            codex_path: codex.display().to_string(),
            cursor,
            resolutions: BTreeMap::new(),
            pending_event: None,
            last_event_at: None,
            last_queued_at: None,
            last_queue_output: None,
            last_error: None,
        }
    }

    #[test]
    fn comment_prompt_is_readable_not_raw_json() {
        let comment = Comment {
            id: 7,
            thread_id: 4,
            body: "make it smaller".into(),
            author: Some("user".into()),
            created_at: 1,
            seen_at: None,
            seen_by: None,
            kind: "comment".into(),
        };
        let thread = Thread {
            id: 4,
            page: "V4".into(),
            target: "intro".into(),
            anchor: "".into(),
            quote: Some("the heading".into()),
            context: None,
            created_at: 1,
            resolved_at: None,
            resolved_by: None,
            working_at: None,
            working_by: None,
        };
        let prompt = comment_message(&comment, &thread, 0);
        assert!(prompt.contains("Page V4 · thread 4 · event 7"));
        assert!(prompt.contains("Feedback:\nmake it smaller"));
        assert!(!prompt.contains("{\""));
    }

    #[test]
    fn explicit_thread_must_be_a_uuid() {
        assert!(resolve_thread(Some("latest")).is_err());
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(resolve_thread(Some(&id)).unwrap(), id);
    }

    #[test]
    fn successful_delivery_advances_and_acks_but_agent_echo_only_advances() {
        let mut store = test_store();
        let (codex, calls) = fake_codex(&store, true);
        assert_eq!(
            discover_codex(Some(&codex)).unwrap(),
            codex.canonicalize().unwrap()
        );
        let (thread, comment) = crate::models::conversation::create_thread(
            &mut store,
                "V4",
                "intro",
                "",
                Some("the heading"),
                None,
                "make it smaller",
                Some("user"),
                "comment",
                &[],
            )
            .unwrap();
        let mut state = state(&codex, 0);
        process_generation(&mut store, &mut state).unwrap();
        assert_eq!(state.cursor, comment);
        assert_eq!(state.pending_event, None);
        assert!(
            state
                .last_queue_output
                .as_deref()
                .unwrap()
                .contains("fake-id")
        );
        let call = std::fs::read_to_string(&calls).unwrap();
        assert!(call.contains("--thread"));
        assert!(call.contains("Sideview comment"));
        assert!(call.contains("Page V4 · thread"));
        let delivered = crate::models::conversation::comments_for_page(&store, "V4").unwrap();
        assert_eq!(delivered[0].seen_by.as_deref(), Some("monitor:codex"));

        let echo = crate::models::conversation::reply(
            &mut store,thread, "done", Some("agent"), "comment", &[])
            .unwrap();
        process_generation(&mut store, &mut state).unwrap();
        assert_eq!(state.cursor, echo);
        assert_eq!(std::fs::read_to_string(&calls).unwrap(), call);
    }

    #[test]
    fn failed_queue_keeps_cursor_and_pending_event_for_retry() {
        let mut store = test_store();
        let (codex, _) = fake_codex(&store, false);
        let (_, comment) = crate::models::conversation::create_thread(
            &mut store,
                "V4",
                "intro",
                "",
                None,
                None,
                "retry me",
                Some("user"),
                "comment",
                &[],
            )
            .unwrap();
        let mut state = state(&codex, 0);
        assert!(process_generation(&mut store, &mut state).is_err());
        assert_eq!(state.cursor, 0);
        let expected = format!("comment:{comment}");
        assert_eq!(state.pending_event.as_deref(), Some(expected.as_str()));
        assert!(
            state
                .last_error
                .as_deref()
                .unwrap()
                .contains("queue refused")
        );
        assert_eq!(crate::models::conversation::comments_for_page(&store, "V4").unwrap()[0].seen_at, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_pid_identity_has_a_start_token() {
        assert!(pid_start(std::process::id()).is_some());
    }
}

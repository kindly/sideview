//! The daemon: serves the page, notices changed page files by polling their
//! mtimes (bindings are the watch list — that's what they're *for*), reparses,
//! and patches the open page over SSE with only the blocks that changed.
//! Long-lived, no idle exit (auto-exit depends on auto-restart, which is
//! precisely what a sandboxed agent cannot do).
//!
//! All render/replay state is in memory, derived from the files — the db
//! holds no content. A new connection always receives the full current state
//! (the client resets on connect), which dissolves `Last-Event-ID` replay,
//! tombstones and the rev counter in one move: at page scale a full resend
//! is cheaper than being clever, and a daemon restart can't strand a client
//! on a rev sequence that no longer exists.

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actix_web::web::{self, Data};
use actix_web::{App, HttpResponse, HttpServer, Responder};
use actix_web_lab::sse;
use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::format;
use crate::netcheck;
use crate::logic::base;
use crate::logic::conversation;
use crate::poll::{block_event, pages_event, poll_loop, threads_event, Outgoing, Shared};
use crate::models::base::{now_ms, DaemonRow, Store};


#[derive(Debug, Clone)]
pub struct Opts {
    /// `--bind auto`: loopback plus the tailnet address when there is one.
    pub bind_auto: bool,
    /// Open the browser once the row is claimed (the auto-spawn "first use"
    /// path; bare foreground `sideview` also uses it).
    pub open_browser: bool,
    /// A pinned port (--port / SIDEVIEW_PORT): configuration is where
    /// address durability belongs — the resurrection test proved the db
    /// isn't (the remembered port dies with it). Pinned means pinned:
    /// bind failure is an error, never a silent ephemeral fallback.
    pub port: Option<u16>,
}

/// A pre-serialized SSE event, fanned out to every connected page.

struct AppState {
    shared: Arc<Mutex<Shared>>,
    root: PathBuf,
    tx: broadcast::Sender<Outgoing>,
    /// For the page's one write (page deletion) and the shutdown clear.
    /// The poll loop has its own connection; handlers otherwise never touch
    /// the store.
    store: Mutex<Store>,
}

#[derive(rust_embed::Embed)]
#[folder = "static/"]
struct Assets;

/// Try to open a browser; failure just means the printed URL is the path.
/// Inside an agent, don't even try — xdg-open needs a desktop session the
/// sandbox doesn't reach.
pub fn open_browser(url: &str) {
    if crate::identity::inside_agent() {
        return;
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn run(store_dir: &Path, opts: &Opts) -> Result<()> {
    let verdict = netcheck::verdict();
    if !verdict.reachable {
        // Started anyway (someone forced it); record the truth so bare
        // `sideview` on the host claims the row instead of deferring to us.
        eprintln!(
            "warning: this daemon is provably unreachable from outside ({}) — recording reachable=0",
            verdict.reasons.join(", ")
        );
    }

    let mut store = Store::open(store_dir)?;

    // Fresh-clone (and fresh-db) rediscovery: pages are files, so a deleted
    // store must not lose them. Committed .sv files re-bind by stem; the
    // throwaway pages under .sideview/pages/ re-bind by their encoded name.
    // The resurrection test (V2.sv's goal) leans on this running first.
    if let Err(e) = rediscover_pages(&store) {
        eprintln!("note: page rediscovery failed: {e:#}");
    }

    // Port: remembered so an open tab reconnects to the same origin across
    // restarts. Taken? Take another and print the new URL. Meta is the
    // durable copy (the row is cleared on clean shutdown); a crash-leftover
    // row is the fallback.
    // Config beats the remembered value: it is canon and survives a deleted
    // db, which is the wrinkle V2's sign-off recorded (the page resurrects;
    // its address did not). An explicit --port still beats both.
    let (cfg, cfg_err) = crate::config::load(&store.root);
    if let Some(e) = &cfg_err {
        eprintln!("config ignored — {e}");
    }
    let (initial_exts, ext_problems) = crate::config::load_extensions(&store.root, &cfg);
    for (path, why) in &ext_problems {
        eprintln!("extension {path} not loaded — {why}");
    }
    let remembered = cfg
        .port
        .or_else(|| store.meta("port").ok().flatten().and_then(|p| p.parse().ok()))
        .or_else(|| base::daemon(&store).ok().flatten().map(|d| d.port))
        .unwrap_or(0);
    let loopback = match opts.port {
        Some(p) => TcpListener::bind((Ipv4Addr::LOCALHOST, p))
            .with_context(|| format!("port {p} is pinned (--port/SIDEVIEW_PORT) but not bindable"))?,
        None => TcpListener::bind((Ipv4Addr::LOCALHOST, remembered))
            .or_else(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
            .context("binding loopback")?,
    };
    let port = loopback.local_addr()?.port();

    let mut listeners = vec![loopback];
    let mut urls = vec![format!("http://127.0.0.1:{port}")];
    if opts.bind_auto {
        for ip in netcheck::tailnet_addrs() {
            // EADDRNOTAVAIL (tailscaled came up after us, or is down): degrade
            // to loopback-only and say so, never die.
            match TcpListener::bind((ip, port)) {
                Ok(l) => {
                    urls.push(match ip {
                        IpAddr::V4(v4) => format!("http://{v4}:{port}"),
                        IpAddr::V6(v6) => format!("http://[{v6}]:{port}"),
                    });
                    listeners.push(l);
                }
                Err(e) => eprintln!("note: tailnet address {ip} not bindable ({e}); loopback only"),
            }
        }
    }

    // Claim the row *after* binding, never before: binding is the step that
    // can fail, and claim-then-bind would evict a healthy daemon on behalf of
    // one that never started.
    let instance_id = uuid::Uuid::new_v4().to_string();
    base::claim_daemon(&mut store, &DaemonRow {
        instance_id: instance_id.clone(),
        pid: std::process::id() as i64,
        port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: now_ms(),
        last_seen: now_ms(),
        ping: 0,
        pong: 0,
        netns: verdict.netns,
        reachable: verdict.reachable,
    })?;
    store.set_meta("port", &port.to_string())?;
    // The bound URLs, durably — so the CLI can print the tailnet addresses
    // instead of them living only in this log.
    store.set_meta("urls", &serde_json::to_string(&urls)?)?;

    for (i, url) in urls.iter().enumerate() {
        if i == 0 {
            eprintln!("local:   {url}");
        } else {
            eprintln!("tailnet: {url}      (any tailnet node can read this)");
        }
    }

    if opts.open_browser {
        open_browser(&format!("http://127.0.0.1:{port}/"));
    }

    let (tx, _) = broadcast::channel::<Outgoing>(1024);
    let shared = Arc::new(Mutex::new(Shared { extensions: initial_exts, ..Shared::default() }));

    // The poll loop gets its own db connection on its own thread; the actix
    // handlers only ever touch the in-memory state.
    {
        let dir = store_dir.to_path_buf();
        let tx = tx.clone();
        let instance_id = instance_id.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            if let Err(e) = poll_loop(&dir, &instance_id, &tx, &shared) {
                eprintln!("poll loop died: {e:#}");
                std::process::exit(1);
            }
        });
    }

    let root = store.root.clone();
    let state = Data::new(AppState { shared, root, tx, store: Mutex::new(store) });

    let server_state = state.clone();
    actix_web::rt::System::new().block_on(async move {
        let mut server = HttpServer::new(move || {
            App::new()
                .app_data(server_state.clone())
                // The Bytes extractor's ceiling — this is the attachment
                // size cap (413 past it), and nothing else reads raw bodies.
                .app_data(web::PayloadConfig::new(ATTACHMENT_CAP))
                .route("/", web::get().to(root_redirect))
                .route("/p/{page}", web::get().to(page))
                // The index: categories and the pages in them.
                .route("/home", web::get().to(page))
                .route("/events", web::get().to(events))
                .route("/api/pages/{page}", web::delete().to(delete_page))
                .route("/api/comments", web::post().to(post_comment))
                .route("/api/source", web::get().to(block_source))
                .route("/api/edit", web::post().to(edit_block))
                .route("/api/attachments", web::post().to(upload_attachment))
                // Extensions (EXTENSIONS.md): the entry with its injections,
                // the extension's own files, and the two call endpoints.
                .route("/x/{ext}/{page}/{block}/__call", web::post().to(ext_call))
                .route("/x/{ext}/{page}/{block}/__call_stream", web::post().to(ext_call_stream))
                .route("/x/{ext}/{page}/{block}/{tail:.*}", web::get().to(ext_serve))
                .route("/api/threads/{id}/resolve", web::post().to(resolve_thread))
                .route("/api/threads/{id}/unresolve", web::post().to(unresolve_thread))
                .route("/f/{path:.*}", web::get().to(project_file))
                .route("/assets/{path:.*}", web::get().to(asset))
        })
        .workers(2)
        // SSE streams never finish on their own, so a graceful drain would
        // stall every shutdown for the full default 30s whenever a tab is
        // open. Nothing in flight is worth draining; drop and let the
        // browser's EventSource reconnect to the next daemon.
        .shutdown_timeout(1);
        for l in listeners {
            server = server.listen(l)?;
        }
        server.run().await
    })?;

    // Ctrl-C lands here: clean shutdown clears the row (only as its holder).
    base::clear_daemon(&state.store.lock().unwrap(), &instance_id)?;
    eprintln!("daemon stopped");
    Ok(())
}

/// The page's one write into the project: tidying power, not authoring power
/// (V1.md) — anyone who can see the page can delete a page, and cannot
/// create or alter content. Deleting a page is deleting its file; the poll
/// loop notices the binding is gone on its next tick and the pages
/// snapshot converges every client.
async fn delete_page(path: web::Path<String>, state: Data<AppState>) -> impl Responder {
    let id = path.into_inner();
    let mut store = state.store.lock().unwrap();
    let Ok(Some(binding)) = base::binding(&store, &id) else {
        return HttpResponse::NotFound().body(format!("no page {id:?}"));
    };
    // Tidying power, not destruction: the page's ✕ deletes a *throwaway*
    // page's file, and merely unbinds anything committed — a promoted .sv
    // as much as an imported DESIGN.md (author, 2026-08-10). Deleting a
    // file someone committed belongs to git, or to an explicit
    // `page rm --file`, never to a two-click affordance in a browser.
    if crate::models::base::is_throwaway_page(&binding.path) {
        let file = state.root.join(&binding.path);
        if let Err(e) = std::fs::remove_file(&file) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return HttpResponse::InternalServerError()
                    .body(format!("removing {}: {e}", file.display()));
            }
        }
        let _ = std::fs::remove_file(file.with_extension("sv.lock"));
    }
    match base::delete_binding(&mut store, &id) {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}

/// Re-find every page file the db doesn't know: committed .sv files bind by
/// stem (V2.sv → "V2"), throwaway pages under .sideview/pages/ by decoding
/// their filename back to the page id. Chip order for rediscovered pages
/// comes from canon: the `order` attribute on `<sv-page>` when the author
/// cares, path order otherwise — binding insertion order carries it.
fn rediscover_pages(store: &Store) -> Result<()> {
    let known: std::collections::HashSet<String> =
        base::bindings(&store)?.into_iter().map(|b| b.path).collect();
    let mut found: Vec<(f64, String, String, String)> = Vec::new(); // (order, path, rel, id)

    let pages_dir = store.dir.join(crate::models::base::PAGES_DIR);
    let mut stack = vec![store.root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                // Hidden trees, dependencies and build output stay unscanned —
                // except the store's own throwaway pages.
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    if path == store.dir || store.dir.starts_with(&path) {
                        stack.push(pages_dir.clone());
                    }
                    continue;
                }
                stack.push(path);
            } else if name.ends_with(".sv") {
                let Ok(rel) = path.strip_prefix(&store.root) else { continue };
                let rel = rel.to_string_lossy().to_string();
                if known.contains(&rel) {
                    continue;
                }
                let stem = name.trim_end_matches(".sv").to_string();
                let id = if path.starts_with(&pages_dir) {
                    // The throwaway filename is the encoded page id.
                    percent_encoding::percent_decode_str(&stem)
                        .decode_utf8_lossy()
                        .to_string()
                } else {
                    stem
                };
                let order = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|src| {
                        format::parse(&src).prop("order").and_then(|o| o.parse::<f64>().ok())
                    })
                    .unwrap_or(f64::MAX);
                found.push((order, rel.clone(), rel, id));
            }
        }
    }

    found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    for (_, _, rel, id) in found {
        if base::binding(&store, &id)?.is_some() {
            eprintln!("note: {rel} not rebound — a different file already holds page id {id:?}");
            continue;
        }
        base::bind_page(&store, &id, &rel, &store.root.display().to_string(), "rediscovered")?;
        eprintln!("rediscovered page {id} ({rel})");
        // Distinct started_at millis keep the canon order stable in the strip.
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

/// 20MB: a phone screenshot is 5, a parquet sample can be more; past it the
/// Bytes extractor answers 413 before the handler runs.
const ATTACHMENT_CAP: usize = 20 * 1024 * 1024;

#[derive(serde::Deserialize)]
struct UploadQuery {
    name: String,
}

/// The upload half of the attachment channel (V3.sv): raw bytes in, the file
/// written under `.sideview/attachments/<sha8>/<name>`, the would-be row
/// handed back. No row is born here — that happens when the comment is sent,
/// so a canceled draft leaves only an unreferenced file for gc.
async fn upload_attachment(
    q: web::Query<UploadQuery>,
    body: web::Bytes,
    state: Data<AppState>,
) -> impl Responder {
    use sha2::Digest as _;

    if body.is_empty() {
        return HttpResponse::BadRequest().body("empty attachment");
    }
    let name = sanitize_filename(&q.name);
    let sha256 = {
        let mut h = sha2::Sha256::new();
        h.update(&body);
        format!("{:x}", h.finalize())
    };
    // Content sniffed, never trusted: the types the card renders inline are
    // identified by magic bytes; everything else falls back to the extension
    // and then to honest octet-stream.
    let mime = sniff_mime(&body, &name);

    let attachments_root = state.store.lock().unwrap().dir.join(conversation::ATTACHMENTS_DIR);
    // <sha8> is the dedupe address; on the astronomical prefix collision
    // (same 8 hex chars, different content, same filename) fall back to the
    // full hash as the directory rather than overwrite.
    let mut dir_name = sha256[..8].to_string();
    let mut abs = attachments_root.join(&dir_name).join(&name);
    if abs.exists() {
        let same = std::fs::read(&abs)
            .map(|existing| {
                let mut h = sha2::Sha256::new();
                h.update(&existing);
                format!("{:x}", h.finalize()) == sha256
            })
            .unwrap_or(false);
        if same {
            return HttpResponse::Ok().json(serde_json::json!({
                "path": format!("{}{}/{}", conversation::ATTACHMENTS_PREFIX, dir_name, name),
                "name": name, "mime": mime, "bytes": body.len(), "sha256": sha256,
            }));
        }
        dir_name = sha256.clone();
        abs = attachments_root.join(&dir_name).join(&name);
    }
    if let Err(e) = std::fs::create_dir_all(abs.parent().unwrap()) {
        return HttpResponse::InternalServerError().body(format!("{e}"));
    }
    // Write-then-rename so a torn upload never leaves a half file at the
    // address a row might later point at.
    let tmp = abs.with_extension("part");
    if let Err(e) = std::fs::write(&tmp, &body).and_then(|_| std::fs::rename(&tmp, &abs)) {
        let _ = std::fs::remove_file(&tmp);
        return HttpResponse::InternalServerError().body(format!("{e}"));
    }
    HttpResponse::Ok().json(serde_json::json!({
        "path": format!("{}{}/{}", conversation::ATTACHMENTS_PREFIX, dir_name, name),
        "name": name, "mime": mime, "bytes": body.len(), "sha256": sha256,
    }))
}

/// Keep the original filename recognizable but never navigable: the last
/// path segment only, path-active and control characters replaced, length
/// bounded, and never empty or a dotfile.
fn sanitize_filename(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let mut name: String = last
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .take(120)
        .collect();
    while name.starts_with('.') {
        name.remove(0);
    }
    if name.is_empty() { "file".to_string() } else { name }
}

/// Magic bytes for what the card renders inline; extension for the rest.
fn sniff_mime(bytes: &[u8], name: &str) -> String {
    let sniffed = match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'%', b'P', b'D', b'F', ..] => Some("application/pdf"),
        _ if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") => Some("image/svg+xml"),
        _ => None,
    };
    match sniffed {
        Some(m) => m.to_string(),
        None => mime_guess::from_path(name).first_raw().unwrap_or("application/octet-stream").to_string(),
    }
}

/// The browser's write path for conversation: the page talks *about* the
/// document, never *as* it. A `thread` field replies; page+target starts a
/// fresh thread at the anchor (threads succeed each other — see V2.sv).
/// Attachments arrive as the rows the upload endpoint handed back — bound
/// here, inside the comment's own transaction.
#[derive(serde::Deserialize)]
struct CommentBody {
    thread: Option<i64>,
    page: Option<String>,
    target: Option<String>,
    #[serde(default)]
    anchor: String,
    quote: Option<String>,
    context: Option<String>,
    body: String,
    /// 'comment' (default) or 'edit' — an edit request on a non-prose block,
    /// the proposal fenced in the body, merged by the agent (V4.sv). The
    /// 'edited' kind is never accepted here: only the splice endpoint mints
    /// those, or a request could forge a "the file changed" record.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    attachments: Vec<conversation::NewAttachment>,
}

async fn post_comment(body: web::Json<CommentBody>, state: Data<AppState>) -> impl Responder {
    let b = body.into_inner();
    if b.body.trim().is_empty() {
        return HttpResponse::BadRequest().body("empty comment body");
    }
    let kind = match b.kind.as_deref() {
        None | Some("comment") => "comment",
        Some("edit") => "edit",
        Some(other) => {
            return HttpResponse::BadRequest().body(format!("kind {other:?} is not postable"))
        }
    };
    let mut store = state.store.lock().unwrap();
    let target = match (b.thread, b.page.as_deref(), b.target.as_deref()) {
        (Some(tid), _, _) => conversation::CommentTarget::Reply { thread: tid },
        (None, Some(page), Some(target)) => conversation::CommentTarget::NewThread {
            page,
            target,
            anchor: &b.anchor,
            quote: b.quote.as_deref(),
            context: b.context.as_deref(),
        },
        _ => return HttpResponse::BadRequest().body("pass thread, or page and target"),
    };
    let result =
        conversation::post_comment(&mut store, target, &b.body, Some("user"), kind, &b.attachments, None);
    match result {
        Ok((thread, id)) => HttpResponse::Ok().json(serde_json::json!({
            "thread": thread, "id": id,
        })),
        Err(e) => HttpResponse::BadRequest().body(format!("{e:#}")),
    }
}

// ---- editing from the page (V4.sv, threads 62–63) ------------------------------

#[derive(serde::Deserialize)]
struct SourceQuery {
    page: String,
    block: String,
}

/// The editor's opening move: the block's raw markdown source plus the hash
/// the save must echo. The page shows rendered HTML; editing needs canon.
async fn block_source(q: web::Query<SourceQuery>, state: Data<AppState>) -> impl Responder {
    let (root, rel) = {
        let store = state.store.lock().unwrap();
        match base::binding(&store, &q.page) {
            Ok(Some(b)) => (store.root.clone(), b.path),
            _ => return HttpResponse::NotFound().body("no such page"),
        }
    };
    // Imported pages have no spliceable blocks — their canon is a foreign
    // file. Refusing here (and in edit_block) keeps format::parse from ever
    // reading an .md as sv, which would hand the editor stray-block soup.
    if crate::config::format_of(&rel, None) != crate::config::Format::Sv {
        return HttpResponse::BadRequest()
            .body(format!("{rel} is an imported page — edit the source file itself"));
    }
    let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
        return HttpResponse::NotFound().body("page file unreadable");
    };
    let page = crate::format::parse(&text);
    let Some(b) = page.blocks.iter().find(|b| b.id() == Some(q.block.as_str())) else {
        return HttpResponse::NotFound().body("no such block");
    };
    HttpResponse::Ok().json(serde_json::json!({
        "type": b.type_name,
        "body": b.body,
        "hash": crate::format::body_hash(&b.body),
    }))
}

#[derive(serde::Deserialize)]
struct EditBody {
    page: String,
    block: String,
    from_hash: String,
    body: String,
}

/// The direct splice — prose only (V4.sv's two tiers: one unclosed tag
/// breaks an html block and html blocks are often generated code, so
/// everything non-prose arrives as a kind='edit' comment instead). Runs
/// under the same sidecar flock the CLI takes, so a browser save and an
/// agent write serialize; the from-hash guard turns the remaining race into
/// a 409 carrying current canon, never a clobber. The page's first
/// authoring power — strip this and /api/comments' kind under any future
/// read-only share.
async fn edit_block(body: web::Json<EditBody>, state: Data<AppState>) -> impl Responder {
        let e = body.into_inner();
    let (root, rel) = {
        let store = state.store.lock().unwrap();
        match base::binding(&store, &e.page) {
            Ok(Some(b)) => (store.root.clone(), b.path),
            _ => return HttpResponse::NotFound().body("no such page"),
        }
    };
    if crate::config::format_of(&rel, None) != crate::config::Format::Sv {
        return HttpResponse::BadRequest()
            .body(format!("{rel} is an imported page — edit the source file itself"));
    }
    let path = root.join(&rel);
    let Ok(lock) = crate::logic::edit::lock_page(&path) else {
        return HttpResponse::InternalServerError().body("could not lock the page");
    };
    let Ok(current) = std::fs::read_to_string(&path) else {
        return HttpResponse::NotFound().body("page file unreadable");
    };
    let page = crate::format::parse(&current);
    let Some(b) = page.blocks.iter().find(|b| b.id() == Some(e.block.as_str())) else {
        return HttpResponse::NotFound().body("no such block");
    };
    if b.type_name != "sv-prose" {
        return HttpResponse::BadRequest()
            .body("only prose splices directly — send the change as an edit comment");
    }
    if crate::format::body_hash(&b.body) != e.from_hash {
        // The agent moved the text meanwhile: hand back current canon so
        // the editor can re-merge — never clobber.
        return HttpResponse::Conflict().json(serde_json::json!({
            "body": b.body,
            "hash": crate::format::body_hash(&b.body),
        }));
    }
    let attrs: Vec<(&str, &str)> = b.attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let block_text = crate::format::block_text("sv-prose", &attrs, &e.body);
    let next = crate::logic::edit::splice(&current, b.lines, Some(&block_text));
    let tmp = path.with_extension("sv.tmp");
    if std::fs::write(&tmp, &next).is_err() || std::fs::rename(&tmp, &path).is_err() {
        return HttpResponse::InternalServerError().body("could not write the page");
    }
    drop(lock);
    // The machine-mail record: how watch stays honest about the file moving
    // under the agent. Only this endpoint mints kind='edited'.
    let mut store = state.store.lock().unwrap();
    if let Err(err) = conversation::record_edited(&mut store, &e.page, &e.block) {
        return HttpResponse::InternalServerError().body(format!("{err:#}"));
    }
    HttpResponse::Ok().json(serde_json::json!({ "hash": crate::format::body_hash(&e.body) }))
}

// ---- extensions (EXTENSIONS.md) -----------------------------------------------
// One generic route family. The entry is served with its injections; the
// extension's other files serve confined to its directory; the two call
// endpoints exec the manifest's binary — fresh process per call, argv array,
// no shell, the caps in ext.rs.

fn ext_lookup(
    state: &AppState,
    ext: &str,
    page: &str,
    block: &str,
) -> std::result::Result<(crate::config::Extension, Vec<(String, String)>, String), HttpResponse> {
    let shared = state.shared.lock().unwrap();
    let Some(x) = shared.extensions.iter().find(|x| x.manifest.name == ext) else {
        return Err(HttpResponse::NotFound().body(format!("no extension {ext:?} installed")));
    };
    // Path segments arrive still percent-encoded (the /p/ route learned the
    // same); page ids can hold `/` and `%`, so decode before the lookup.
    let page = percent_encoding::percent_decode_str(page).decode_utf8_lossy().to_string();
    let block = percent_encoding::percent_decode_str(block).decode_utf8_lossy().to_string();
    let Some((attrs, body)) =
        shared.pages.get(&page).and_then(|p| p.ext_blocks.get(&block)).cloned()
    else {
        return Err(HttpResponse::NotFound()
            .body(format!("no {} block {block:?} on page {page:?}", x.tag())));
    };
    Ok((x.clone(), attrs, body))
}

async fn ext_serve(
    path: web::Path<(String, String, String, String)>,
    state: Data<AppState>,
) -> impl Responder {
    let (ext, page, block, tail) = path.into_inner();
    let (x, attrs, body) = match ext_lookup(&state, &ext, &page, &block) {
        Ok(v) => v,
        Err(r) => return r,
    };
    // The entry gets the injections; everything else is a plain file.
    if tail.is_empty() || tail == x.manifest.entry {
        let entry_path = state.root.join(&x.dir).join(&x.manifest.entry);
        let html = match std::fs::read_to_string(&entry_path) {
            Ok(h) => h,
            Err(e) => {
                return HttpResponse::NotFound()
                    .body(format!("extension entry {}: {e}", entry_path.display()))
            }
        };
        let page_dec = percent_encoding::percent_decode_str(&page).decode_utf8_lossy();
        let block_dec = percent_encoding::percent_decode_str(&block).decode_utf8_lossy();
        let base = format!(
            "/x/{}/{}/{}/",
            x.manifest.name,
            crate::identity::encode(&page_dec),
            crate::identity::encode(&block_dec)
        );
        let json = crate::ext::block_json(&page_dec, &block_dec, &attrs, &body);
        return HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .insert_header(("Cache-Control", "no-cache"))
            .body(crate::ext::inject_entry(&html, &base, &json));
    }
    match crate::ext::safe_ext_file(&state.root, &x, &tail) {
        Some(file) => match std::fs::read(&file) {
            Ok(bytes) => HttpResponse::Ok()
                .content_type(mime_guess::from_path(&file).first_or_octet_stream().to_string())
                .insert_header(("Cache-Control", "no-cache"))
                .body(bytes),
            Err(e) => HttpResponse::NotFound().body(format!("{e}")),
        },
        None => HttpResponse::NotFound().body("outside the extension directory"),
    }
}

async fn ext_call(
    path: web::Path<(String, String, String)>,
    body: web::Json<crate::ext::CallBody>,
    state: Data<AppState>,
) -> impl Responder {
    let (ext, page, block) = path.into_inner();
    let (x, _, _) = match ext_lookup(&state, &ext, &page, &block) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let bin = match crate::ext::resolve_bin(&state.root, &x) {
        Ok(b) => b,
        Err(e) => return HttpResponse::BadRequest().body(format!("{e:#}")),
    };
    let b = body.into_inner();
    match crate::ext::run_call(&bin, &b.args, b.stdin.as_deref(), &state.root).await {
        Ok(result) => HttpResponse::Ok().json(result),
        // Mechanism failures (spawn, timeout, cap) — a non-zero *exit* is a
        // result, handled above by resolving normally.
        Err(e) => HttpResponse::BadGateway().body(format!("{e:#}")),
    }
}

async fn ext_call_stream(
    path: web::Path<(String, String, String)>,
    body: web::Json<crate::ext::CallBody>,
    state: Data<AppState>,
) -> impl Responder {
    let (ext, page, block) = path.into_inner();
    let (x, _, _) = match ext_lookup(&state, &ext, &page, &block) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let bin = match crate::ext::resolve_bin(&state.root, &x) {
        Ok(b) => b,
        Err(e) => return HttpResponse::BadRequest().body(format!("{e:#}")),
    };
    let b = body.into_inner();
    match crate::ext::stream_call(bin, b.args, b.stdin, state.root.clone(), x.manifest.name.clone()) {
        Ok(stream) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .insert_header(("X-Accel-Buffering", "no"))
            .streaming(stream),
        Err(e) => HttpResponse::BadGateway().body(format!("{e:#}")),
    }
}

async fn resolve_thread(path: web::Path<i64>, state: Data<AppState>) -> impl Responder {
    set_resolution(path.into_inner(), false, &state)
}

async fn unresolve_thread(path: web::Path<i64>, state: Data<AppState>) -> impl Responder {
    set_resolution(path.into_inner(), true, &state)
}

/// Resolve is undoable and idempotent from the page's point of view: asking
/// for a state the thread already holds is success, not conflict.
fn set_resolution(id: i64, undo: bool, state: &Data<AppState>) -> HttpResponse {
    let mut store = state.store.lock().unwrap();
    match conversation::resolve(&mut store, id, Some("user"), undo, None) {
        Ok(None) => HttpResponse::NotFound().body(format!("no thread {id}")),
        Ok(Some(_)) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}


fn to_sse(o: Outgoing) -> sse::Event {
    sse::Event::Data(sse::Data::new(o.data).event(o.kind))
}

/// `/` — the server owns routing now (author, 2026-08-18): redirect to the
/// most recently active page, which is what the client's auto-follow used
/// to decide and the URL now simply *is*. An empty project gets the shell,
/// which renders its honest "no pages yet".
async fn root_redirect(req: actix_web::HttpRequest, state: Data<AppState>) -> HttpResponse {
    let most_active = {
        let shared = state.shared.lock().unwrap();
        shared
            .order
            .iter()
            .max_by_key(|(_, at)| *at)
            .map(|(id, _)| id.clone())
    };
    match most_active {
        Some(id) => HttpResponse::Found()
            .insert_header(("Location", format!("/p/{}", crate::identity::encode(&id))))
            // The choice changes as pages become active: never cache it.
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
        // Empty project: serve the page shell directly ('/' has no page
        // in its match info, so the title is just the project's).
        None => page(req, state).await,
    }
}

async fn page(req: actix_web::HttpRequest, state: Data<AppState>) -> HttpResponse {
    match Assets::get("index.html") {
        Some(f) => {
            // no-cache asks politely; iOS sometimes pairs a fresh script with
            // stale CSS anyway (bit live twice — HANDOFF's mobile saga, then
            // thread 35). A per-daemon-start stamp on the asset URLs makes
            // every restart a hard bust, and a restart is already how a new
            // binary arrives.
            static STAMP: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
            let v = STAMP.get_or_init(crate::models::base::now_ms);
            // The title carries the page and the project, because browser
            // history is where titles live (author, 2026-08-23): several
            // projects' daemons all titled "sideview" were indistinguishable
            // there. Server-side because navigation is (0.3.1): the title a
            // page is SERVED with is the one history records.
            let project = state
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // A project literally named "sideview" would double the wordmark.
            let suffix = if project == "sideview" || project.is_empty() {
                "sideview".to_string()
            } else {
                format!("{project} — sideview")
            };
            let title = match req.match_info().get("page") {
                Some(id) => {
                    let label = state
                        .shared
                        .lock()
                        .unwrap()
                        .pages
                        .get(id)
                        .and_then(|p| p.props.get("label"))
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| id.to_string());
                    format!("{label} · {suffix}")
                }
                None => suffix,
            };
            let html = String::from_utf8_lossy(&f.data)
                .replace(
                    "<title>sideview</title>",
                    &format!("<title>{}</title>", sideview_blocks::render::text_escape(&title)),
                )
                .replace("/assets/sideview.css", &format!("/assets/sideview.css?v={v}"))
                .replace("/assets/app.js", &format!("/assets/app.js?v={v}"));
            HttpResponse::Ok().content_type("text/html; charset=utf-8").body(html)
        }
        None => HttpResponse::InternalServerError().body("index.html missing from binary"),
    }
}

async fn asset(path: web::Path<String>) -> impl Responder {
    let rel = path.into_inner();
    match Assets::get(&rel) {
        Some(f) => HttpResponse::Ok()
            .content_type(
                mime_guess::from_path(&rel).first_or_octet_stream().to_string(),
            )
            // Embedded assets change on every binary upgrade with unchanged
            // URLs, and phones cache aggressively: force revalidation.
            .insert_header(("Cache-Control", "no-cache"))
            // Public static assets, deliberately importable from the
            // opaque-origin srcdoc iframes: ESM imports are CORS-gated, and
            // this one header is what lets an html block be a Vue island.
            .insert_header(("Access-Control-Allow-Origin", "*"))
            .body(f.data.into_owned()),
        None => HttpResponse::NotFound().body(format!("no embedded asset {rel}")),
    }
}

/// One long-lived stream per page. Every connection starts with the full
/// current state — the client resets on connect — so reconnection after any
/// gap (sleep, daemon restart, lagged stream) converges by construction.
async fn events(state: Data<AppState>) -> actix_web::Result<impl Responder> {
    // Subscribe before snapshotting: an event landing in between is delivered
    // twice, and upserts are idempotent; the other order loses it.
    let rx = state.tx.subscribe();

    let mut replay: Vec<sse::Event> = Vec::new();
    {
        let shared = state.shared.lock().unwrap();
        replay.push(to_sse(pages_event(&shared)));
        for (id, _) in &shared.order {
            if let Some(page) = shared.pages.get(id) {
                for b in &page.blocks {
                    replay.push(to_sse(block_event(id, b)));
                }
            }
        }
        for json in shared.conversations.values() {
            replay.push(to_sse(threads_event(json.clone())));
        }
    }

    let stream = futures_util::stream::iter(replay)
        .chain(live_events(rx))
        .map(Ok::<_, Infallible>);

    Ok(sse::Sse::from_stream(stream)
        .with_keep_alive(Duration::from_secs(15))
        .customize()
        .insert_header(("X-Accel-Buffering", "no")))
}

/// Only paths inside the project root, ever. Resolve symlinks, compare
/// against the root, and return a clear error rather than a 404 so a
/// mistyped path is diagnosable.
async fn project_file(path: web::Path<String>, state: Data<AppState>) -> impl Responder {
    let rel = path.into_inner();
    if rel.starts_with('/') || rel.split('/').any(|c| c == "..") {
        return HttpResponse::Forbidden()
            .body(format!("refusing {rel:?}: paths must be relative, inside the project"));
    }
    let root = match state.root.canonicalize() {
        Ok(r) => r,
        Err(e) => return HttpResponse::InternalServerError().body(format!("project root: {e}")),
    };
    let full = match root.join(&rel).canonicalize() {
        Ok(f) => f,
        Err(_) => {
            return HttpResponse::NotFound()
                .body(format!("no file {rel:?} under {}", root.display()))
        }
    };
    if !full.starts_with(&root) {
        return HttpResponse::Forbidden().body(format!(
            "refusing {rel:?}: resolves outside the project root ({})",
            full.display()
        ));
    }
    // The store's internals are not project content — bindings and the daemon
    // row are nobody's business over the tailnet. Only the named internals
    // are refused: other files under .sideview/ still serve (pages, and the
    // dogfood comparison pages iframe from there).
    if let Ok(store_dir) = root.join(crate::models::base::DIR_NAME).canonicalize() {
        if full.starts_with(&store_dir) {
            let name = full.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with(crate::models::base::DB_FILE)
                || name == "daemon.log"
                || name == crate::models::base::SPAWN_LOCK
            {
                return HttpResponse::Forbidden()
                    .body(format!("refusing {rel:?}: the store's internals are not served"));
            }
        }
    }
    match std::fs::read(&full) {
        Ok(bytes) => HttpResponse::Ok()
            .content_type(mime_guess::from_path(&full).first_or_octet_stream().to_string())
            .body(bytes),
        Err(e) => HttpResponse::NotFound().body(format!("reading {rel:?}: {e}")),
    }
}

/// The live half of an SSE stream. A subscriber that falls behind the
/// broadcast buffer gets `Err(Lagged)` — it has *lost events*, so the stream
/// must end rather than resume: a dropped stream makes `EventSource`
/// reconnect, and the full-state connection heals the gap. Skipping the
/// error and carrying on would silently desynchronize the page.
fn live_events(
    rx: broadcast::Receiver<Outgoing>,
) -> impl futures_util::Stream<Item = sse::Event> {
    BroadcastStream::new(rx)
        .take_while(|r| std::future::ready(r.is_ok()))
        .filter_map(|r| async move { r.ok().map(to_sse) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagged_subscriber_stream_ends_instead_of_resuming() {
        actix_web::rt::System::new().block_on(async {
            let (tx, rx) = broadcast::channel::<Outgoing>(1);
            // Three sends into a one-slot buffer: the receiver has lost events.
            for _ in 0..3 {
                tx.send(Outgoing { kind: "block", data: String::new() }).unwrap();
            }
            drop(tx);
            let got: Vec<_> = live_events(rx).collect().await;
            assert_eq!(
                got.len(),
                0,
                "a lagged stream must terminate (forcing a fresh full-state connection), not skip ahead"
            );
        });
    }

    #[actix_web::test]
    async fn file_endpoint_refuses_store_internals_but_serves_neighbours() {
        let dir = std::env::temp_dir().join(format!("sv-fe-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        std::fs::write(store.root.join("ok.html"), "<p>ok</p>").unwrap();
        std::fs::write(store.dir.join("lab.html"), "<p>lab</p>").unwrap();
        std::fs::write(store.dir.join("sideview.db-pre-v4"), "backup").unwrap();
        std::fs::write(store.dir.join(crate::models::base::SPAWN_LOCK), "").unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/f/{path:.*}", web::get().to(project_file)),
        )
        .await;
        for (path, expect_ok) in [
            ("/f/ok.html", true),
            ("/f/.sideview/lab.html", true),                // labs iframe from here
            ("/f/.sideview/sideview.db", false),            // bindings + daemon row
            ("/f/.sideview/sideview.db-pre-v4", false),     // backups too
            ("/f/.sideview/spawn.lock", false),
        ] {
            let req = actix_web::test::TestRequest::get().uri(path).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            // Absent internals must still refuse, not 404 into existence checks.
            let ok = res.status().is_success();
            let refused = res.status() == actix_web::http::StatusCode::FORBIDDEN;
            if expect_ok {
                assert!(ok, "{path} should serve, got {}", res.status());
            } else {
                assert!(refused, "{path} must be forbidden, got {}", res.status());
            }
        }
    }

    #[actix_web::test]
    async fn delete_page_removes_file_and_binding_and_404s_on_unknown() {
        let dir = std::env::temp_dir().join(format!("sv-del-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        base::bind_page(&store, "s1", ".sideview/pages/s1.sv", "/tmp", "test").unwrap();
        let file = store.pages_dir().unwrap().join("s1.sv");
        std::fs::write(&file, "<sv-prose id=\"b1\">\nx\n</sv-prose>\n").unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/pages/{page}", web::delete().to(delete_page)),
        )
        .await;

        let req = actix_web::test::TestRequest::delete().uri("/api/pages/nope").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = actix_web::test::TestRequest::delete().uri("/api/pages/s1").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NO_CONTENT);
        assert!(!file.exists(), "the page file is deleted");
        assert!(
            state.store.lock().unwrap().binding("s1").unwrap().is_none(),
            "the binding is deleted"
        );

        // A committed page is a different tier: the ✕ closes it and leaves
        // the file, because deleting something someone committed is git's
        // job or an explicit `page rm --file`, never a browser affordance.
        let doc = state.store.lock().unwrap().root.join("DESIGN.md");
        std::fs::write(&doc, "# Design\n").unwrap();
        state.store.lock().unwrap().bind_page("DESIGN", "DESIGN.md", "/tmp", "config").unwrap();
        let req = actix_web::test::TestRequest::delete().uri("/api/pages/DESIGN").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NO_CONTENT);
        assert!(doc.exists(), "a committed file survives the page's ✕");
        assert!(
            state.store.lock().unwrap().binding("DESIGN").unwrap().is_none(),
            "…but it is unbound"
        );
    }

    #[test]
    fn rediscovery_binds_committed_and_throwaway_pages_in_canon_order() {
        let dir = std::env::temp_dir().join(format!("sv-rd-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        // Two committed pages, order attribute inverting path order…
        std::fs::write(store.root.join("zebra.sv"), "<sv-page order=\"1\">\n</sv-page>\n").unwrap();
        std::fs::write(store.root.join("alpha.sv"), "<sv-page order=\"2\">\n</sv-page>\n").unwrap();
        // …and a throwaway whose filename encodes its page id.
        std::fs::write(
            store.pages_dir().unwrap().join("cwd%3A%2Ftmp%2Fp.sv"),
            "<sv-prose id=\"b1\">\nx\n</sv-prose>\n",
        )
        .unwrap();
        rediscover_pages(&store).unwrap();
        let bindings = base::bindings(&store).unwrap();
        let ids: Vec<&str> = bindings.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["zebra", "alpha", "cwd:/tmp/p"],
            "order attr beats path order; throwaway ids decode"
        );
        // Idempotent: a second scan binds nothing new.
        rediscover_pages(&store).unwrap();
        assert_eq!(base::bindings(&store).unwrap().len(), 3);
    }

    #[actix_web::test]
    async fn comment_endpoint_creates_threads_replies_and_resolves() {
        let dir = std::env::temp_dir().join(format!("sv-cm-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/comments", web::post().to(post_comment))
                .route("/api/threads/{id}/resolve", web::post().to(resolve_thread))
                .route("/api/threads/{id}/unresolve", web::post().to(unresolve_thread)),
        )
        .await;

        // First comment creates its thread…
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "v2", "target": "b3", "anchor": "p:3f9c2a1b04d2",
                "quote": "the paragraph…", "body": "yay complete"
            }))
            .to_request();
        let res: serde_json::Value =
            actix_web::test::call_and_read_body_json(&app, req).await;
        let thread = res["thread"].as_i64().unwrap();

        // …a reply names the thread, anchor-free…
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({ "thread": thread, "body": "second thoughts" }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert!(res.status().is_success());

        // …resolve round-trips, and an unknown thread 404s.
        for (uri, expect) in [
            (format!("/api/threads/{thread}/resolve"), 204u16),
            (format!("/api/threads/{thread}/unresolve"), 204),
            ("/api/threads/999/resolve".to_string(), 404),
        ] {
            let req = actix_web::test::TestRequest::post().uri(&uri).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            assert_eq!(res.status().as_u16(), expect, "{uri}");
        }
        let store = state.store.lock().unwrap();
        assert_eq!(crate::models::conversation::comments_for_page(&store, "v2").unwrap().len(), 2);
        assert!(crate::models::conversation::threads_for_page(&store, "v2").unwrap()[0].resolved_at.is_none());
    }

    #[actix_web::test]
    async fn page_title_names_the_page_and_the_project() {
        let dir = std::env::temp_dir().join(format!("sv-ti-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        let mut shared = Shared::default();
        let mut labeled = crate::poll::PageState::default();
        labeled.props.insert("label".into(), serde_json::json!("Parser <plan>"));
        shared.pages.insert("v9".into(), labeled);
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(shared)),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/home", web::get().to(page))
                .route("/p/{page}", web::get().to(page)),
        )
        .await;
        let project = state.root.file_name().unwrap().to_string_lossy().to_string();

        // History is where titles live: every serve names the project…
        let body = actix_web::test::call_and_read_body(
            &app,
            actix_web::test::TestRequest::get().uri("/home").to_request(),
        )
        .await;
        let html = String::from_utf8_lossy(&body).to_string();
        assert!(html.contains(&format!("<title>{project} — sideview</title>")), "{html:.300}");

        // …a labeled page leads with its label, escaped…
        let body = actix_web::test::call_and_read_body(
            &app,
            actix_web::test::TestRequest::get().uri("/p/v9").to_request(),
        )
        .await;
        let html = String::from_utf8_lossy(&body).to_string();
        assert!(
            html.contains(&format!("<title>Parser &lt;plan&gt; · {project} — sideview</title>")),
            "label leads, escaped"
        );

        // …and an unlabeled or unknown page falls back to its id.
        let body = actix_web::test::call_and_read_body(
            &app,
            actix_web::test::TestRequest::get().uri("/p/scratch").to_request(),
        )
        .await;
        let html = String::from_utf8_lossy(&body).to_string();
        assert!(html.contains(&format!("<title>scratch · {project} — sideview</title>")));
    }

    #[actix_web::test]
    async fn edit_endpoints_source_splice_guard_and_request() {
        let dir = std::env::temp_dir().join(format!("sv-ed-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        std::fs::write(
            store.root.join("plan.sv"),
            "<sv-page>\n\n<sv-prose id=\"b1\">\nold text\n</sv-prose>\n\n<sv-markup id=\"b2\">\n<div>card</div>\n</sv-markup>\n\n</sv-page>\n",
        )
        .unwrap();
        base::bind_page(&store, "plan", "plan.sv", "/tmp", "test").unwrap();
        std::fs::write(store.root.join("NOTES.md"), "# imported\n\nnot spliceable\n").unwrap();
        base::bind_page(&store, "NOTES", "NOTES.md", "/tmp", "test").unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/source", web::get().to(block_source))
                .route("/api/edit", web::post().to(edit_block))
                .route("/api/comments", web::post().to(post_comment)),
        )
        .await;

        // The editor's opening move: raw source plus the hash to echo.
        let req = actix_web::test::TestRequest::get()
            .uri("/api/source?page=plan&block=b1")
            .to_request();
        let src: serde_json::Value = actix_web::test::call_and_read_body_json(&app, req).await;
        assert_eq!(src["type"], "sv-prose");
        assert_eq!(src["body"], "old text");
        let hash = src["hash"].as_str().unwrap().to_string();

        // A stale hash 409s with current canon — never a clobber.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/edit")
            .set_json(serde_json::json!({
                "page": "plan", "block": "b1", "from_hash": "0000000000000000", "body": "clobber"
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status().as_u16(), 409);

        // The honest hash splices, and the machine-mail record is minted.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/edit")
            .set_json(serde_json::json!({
                "page": "plan", "block": "b1", "from_hash": hash, "body": "new **text**"
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert!(res.status().is_success());
        {
            let store = state.store.lock().unwrap();
            let file = std::fs::read_to_string(store.root.join("plan.sv")).unwrap();
            assert!(file.contains("new **text**") && !file.contains("old text"));
            let comments = crate::models::conversation::comments_for_page(&store, "plan").unwrap();
            assert_eq!(comments.len(), 1);
            assert_eq!(comments[0].kind, "edited", "only the splice endpoint mints these");
        }

        // Imported pages refuse BOTH endpoints — their canon is a foreign
        // file, and parsing .md as sv would hand the editor stray soup
        // (found live on a bound .md, 2026-08-23). The file stays untouched.
        for req in [
            actix_web::test::TestRequest::get()
                .uri("/api/source?page=NOTES&block=b1")
                .to_request(),
            actix_web::test::TestRequest::post()
                .uri("/api/edit")
                .set_json(serde_json::json!({
                    "page": "NOTES", "block": "b1", "from_hash": "x", "body": "mangle"
                }))
                .to_request(),
        ] {
            let res = actix_web::test::call_service(&app, req).await;
            assert_eq!(res.status().as_u16(), 400, "imported pages are not page-editable");
        }
        {
            let store = state.store.lock().unwrap();
            let md = std::fs::read_to_string(store.root.join("NOTES.md")).unwrap();
            assert_eq!(md, "# imported\n\nnot spliceable\n", "the .md was never touched");
        }

        // Non-prose refuses the splice — that tier is the edit request…
        let req = actix_web::test::TestRequest::post()
            .uri("/api/edit")
            .set_json(serde_json::json!({
                "page": "plan", "block": "b2", "from_hash": "x", "body": "nope"
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status().as_u16(), 400);

        // …which posts as kind='edit' through the ordinary comment door.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "plan", "target": "b2", "quote": "edit b2",
                "body": "```md\nmake the card an alert\n```", "kind": "edit"
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert!(res.status().is_success());
        // 'edited' is never postable: a request must not forge the record.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "plan", "target": "b2", "body": "forged", "kind": "edited"
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status().as_u16(), 400);
        let store = state.store.lock().unwrap();
        let kinds: Vec<String> =
            crate::models::conversation::comments_for_page(&store, "plan").unwrap().into_iter().map(|c| c.kind).collect();
        assert_eq!(kinds, vec!["edited".to_string(), "edit".to_string()]);
    }

    #[actix_web::test]
    async fn attachments_upload_dedupe_bind_and_refuse_impostors() {
        let dir = std::env::temp_dir().join(format!("sv-at-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/api/attachments", web::post().to(upload_attachment))
                // Extensions (EXTENSIONS.md): the entry with its injections,
                // the extension's own files, and the two call endpoints.
                .route("/x/{ext}/{page}/{block}/__call", web::post().to(ext_call))
                .route("/x/{ext}/{page}/{block}/__call_stream", web::post().to(ext_call_stream))
                .route("/x/{ext}/{page}/{block}/{tail:.*}", web::get().to(ext_serve))
                .route("/api/comments", web::post().to(post_comment)),
        )
        .await;

        // Upload writes the file, sniffs the type, hands back the row-to-be.
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let req = actix_web::test::TestRequest::post()
            .uri("/api/attachments?name=../evil/sh%20ot.png")
            .set_payload(png.clone())
            .to_request();
        let a: serde_json::Value = actix_web::test::call_and_read_body_json(&app, req).await;
        assert_eq!(a["name"], "sh ot.png", "last path segment only — traversal shed at the door");
        assert_eq!(a["mime"], "image/png", "magic bytes, not the claimed extension's word");
        let rel = a["path"].as_str().unwrap().to_string();
        assert!(rel.starts_with(conversation::ATTACHMENTS_PREFIX));
        let abs = state.store.lock().unwrap().root.join(&rel);
        assert!(abs.is_file());

        // Same bytes again: the same address comes back, nothing rewritten.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/attachments?name=../evil/sh%20ot.png")
            .set_payload(png)
            .to_request();
        let b: serde_json::Value = actix_web::test::call_and_read_body_json(&app, req).await;
        assert_eq!(b["path"], rel, "dedupe: same bytes, one file");

        // The comment binds the row; the snapshot carries it.
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "v3", "target": "b1", "body": "see attached",
                "attachments": [a],
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert!(res.status().is_success());
        {
            let store = state.store.lock().unwrap();
            let atts = crate::models::conversation::attachments_for_page(&store, "v3").unwrap();
            assert_eq!(atts.len(), 1);
            assert_eq!(atts[0].path, rel);
            let json = crate::poll::conversation_json(&store, "v3").unwrap();
            assert!(json.contains("\"attachments\""), "snapshot carries the third list");
            assert!(json.contains("\"body_html\""), "comments travel rendered beside their source");
        }

        // A fabricated path — a real project file — is refused before it can
        // become a row that page rm would one day unlink.
        std::fs::write(state.store.lock().unwrap().root.join("canon.rs"), "keep").unwrap();
        let req = actix_web::test::TestRequest::post()
            .uri("/api/comments")
            .set_json(serde_json::json!({
                "page": "v3", "target": "b1", "body": "impostor",
                "attachments": [{"path": "canon.rs", "name": "canon.rs",
                                  "mime": "text/plain", "bytes": 4, "sha256": "cc"}],
            }))
            .to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status().as_u16(), 400);
    }

    #[actix_web::test]
    async fn extension_frames_serve_injected_and_calls_exec_the_manifest_bin() {
        let dir = std::env::temp_dir().join(format!("sv-x-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        let ext_dir = store.root.join("extensions/demo");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("index.html"), "<head><title>d</title></head><body>x</body>").unwrap();
        std::fs::write(ext_dir.join("style.css"), "body{}").unwrap();
        std::fs::write(store.root.join("secret.txt"), "canon").unwrap();

        let ext = crate::config::Extension {
            manifest: crate::config::Manifest {
                name: "demo".into(),
                api: 1,
                render: "frame".into(),
                entry: "index.html".into(),
                bin: Some("cat".into()),
            },
            dir: "extensions/demo".into(),
        };
        let mut page = crate::poll::PageState::default();
        page.ext_blocks.insert(
            "b1".into(),
            (vec![("db".into(), "x.duckdb".into())], "select 1".into()),
        );
        let shared = Shared {
            extensions: vec![ext],
            pages: std::collections::HashMap::from([("v3".to_string(), page)]),
            ..Shared::default()
        };
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(shared)),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state.clone())
                .route("/x/{ext}/{page}/{block}/__call", web::post().to(ext_call))
                .route("/x/{ext}/{page}/{block}/{tail:.*}", web::get().to(ext_serve)),
        )
        .await;

        // The entry arrives with its injections, in order.
        let req = actix_web::test::TestRequest::get().uri("/x/demo/v3/b1/").to_request();
        let body = actix_web::test::call_and_read_body(&app, req).await;
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains(r#"<base href="/x/demo/v3/b1/">"#), "base: {html}");
        assert!(html.contains(r#""db":"x.duckdb""#) && html.contains("select 1"), "block context");
        assert!(html.contains("window.sideview"), "the api");
        assert!(html.find("<base").unwrap() < html.find("<title>").unwrap(), "prelude first");

        // Its own files serve; the project outside its directory does not.
        let ok = actix_web::test::TestRequest::get().uri("/x/demo/v3/b1/style.css").to_request();
        assert!(actix_web::test::call_service(&app, ok).await.status().is_success());
        let esc = actix_web::test::TestRequest::get()
            .uri("/x/demo/v3/b1/../../secret.txt")
            .to_request();
        assert_ne!(
            actix_web::test::call_service(&app, esc).await.status().as_u16(),
            200,
            "confinement"
        );

        // __call execs the manifest's bin — args from the caller, stdin carried.
        let req = actix_web::test::TestRequest::post()
            .uri("/x/demo/v3/b1/__call")
            .set_json(serde_json::json!({"args": [], "stdin": "hello"}))
            .to_request();
        let r: serde_json::Value = actix_web::test::call_and_read_body_json(&app, req).await;
        assert_eq!((r["code"].as_i64(), r["stdout"].as_str()), (Some(0), Some("hello")));

        // Unknown extension and unknown block are named 404s.
        let req = actix_web::test::TestRequest::get().uri("/x/nope/v3/b1/").to_request();
        assert_eq!(actix_web::test::call_service(&app, req).await.status().as_u16(), 404);
        let req = actix_web::test::TestRequest::get().uri("/x/demo/v3/b9/").to_request();
        assert_eq!(actix_web::test::call_service(&app, req).await.status().as_u16(), 404);
    }

    /// Page ids can contain `/` and `%` (cwd and tmux rungs). The printed
    /// URLs percent-encode them; this pins that the encoded form actually
    /// matches the single-segment page route.
    #[actix_web::test]
    async fn encoded_page_ids_match_the_page_route() {
        let dir = std::env::temp_dir().join(format!("sv-en-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::models::base::DIR_NAME)).unwrap();
        let (tx, _) = broadcast::channel(8);
        let state = Data::new(AppState {
            shared: Arc::new(Mutex::new(Shared::default())),
            root: store.root.clone(),
            tx,
            store: Mutex::new(store),
        });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/p/{page}", actix_web::web::get().to(page)),
        )
        .await;
        for uri in ["/p/cwd%3A%2Fhome%2Fdavid%2Fproj", "/p/tmux%2542"] {
            let req = actix_web::test::TestRequest::get().uri(uri).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            assert!(res.status().is_success(), "{uri} did not match the page route");
        }
    }
}

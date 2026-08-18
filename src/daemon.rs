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

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use actix_web::web::{self, Data};
use actix_web::{App, HttpResponse, HttpServer, Responder};
use actix_web_lab::sse;
use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::format;
use crate::netcheck;
use crate::render;
use crate::store::{now_ms, DaemonRow, Store};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEARTBEAT_EVERY: u32 = 12; // × POLL_INTERVAL ≈ 3s

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
#[derive(Debug, Clone)]
struct Outgoing {
    kind: &'static str, // "block" | "sessions" | "threads"
    data: String,
}

/// One rendered block as the page sees it. Comparing these decides whether an
/// upsert goes out — html, position and headings are all part of "changed".
#[derive(Debug, Clone, PartialEq)]
struct Rendered {
    id: String,
    ord: String,
    html: String,
    headings_json: serde_json::Value,
}

/// Everything the daemon knows about one session's page, derived from its
/// file. Rebuilt from scratch whenever the file changes; never persisted.
#[derive(Debug, Clone, Default)]
struct PageState {
    props: serde_json::Map<String, serde_json::Value>,
    blocks: Vec<Rendered>,
    /// (mtime, len) of the file this state was derived from.
    stamp: Option<(SystemTime, u64)>,
    /// Extension blocks' raw context — attrs and body — kept for the
    /// SIDEVIEW_BLOCK injection when the frame is served. Only blocks whose
    /// tag matched a registered extension appear here.
    ext_blocks: HashMap<String, (Vec<(String, String)>, String)>,
    /// Files the page's blocks reference (sv-csv src=), with the stamps they
    /// were rendered from — the poll loop stats these too, so overwriting a
    /// referenced CSV re-renders its block: reference-never-embed made live.
    file_refs: Vec<(String, Option<(SystemTime, u64)>)>,
}

#[derive(Default)]
struct Shared {
    /// Binding order (most recently active first), as of the last poll.
    sessions: Vec<(String, i64)>,
    pages: HashMap<String, PageState>,
    /// Per-page conversation snapshots (threads + comments), pre-serialized.
    /// Page-scale full resends, same trust story as block replay: the client
    /// resets on connect and every change ships the whole page's conversation.
    conversations: HashMap<String, String>,
    /// Explicit outlines (page → parsed spec), riding the sessions event as a
    /// prop so the client needs no fourth event kind.
    outlines: HashMap<String, serde_json::Value>,
    /// Installed extensions, loaded from config (reloaded when it changes).
    extensions: Vec<crate::config::Extension>,
}

struct AppState {
    shared: Arc<Mutex<Shared>>,
    root: PathBuf,
    tx: broadcast::Sender<Outgoing>,
    /// For the page's one write (session deletion) and the shutdown clear.
    /// The poll loop has its own connection; handlers otherwise never touch
    /// the store.
    store: Mutex<Store>,
}

#[derive(rust_embed::Embed)]
#[folder = "static/"]
struct Assets;

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
        .or_else(|| store.daemon().ok().flatten().map(|d| d.port))
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
    store.claim_daemon(&DaemonRow {
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
        crate::cli::open_browser(&format!("http://127.0.0.1:{port}/"));
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
                .route("/s/{session}", web::get().to(page))
                // The index: categories and the pages in them.
                .route("/home", web::get().to(page))
                .route("/events", web::get().to(events))
                .route("/api/pages/{page}", web::delete().to(delete_session))
                // The old noun, one release of grace — same handler.
                .route("/api/sessions/{session}", web::delete().to(delete_session))
                .route("/api/comments", web::post().to(post_comment))
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
    state.store.lock().unwrap().clear_daemon(&instance_id)?;
    eprintln!("daemon stopped");
    Ok(())
}

/// The page's one write into the project: tidying power, not authoring power
/// (V1.md) — anyone who can see the page can delete a session, and cannot
/// create or alter content. Deleting a page is deleting its file; the poll
/// loop notices the binding is gone on its next tick and the sessions
/// snapshot converges every client.
async fn delete_session(path: web::Path<String>, state: Data<AppState>) -> impl Responder {
    let id = path.into_inner();
    let mut store = state.store.lock().unwrap();
    let Ok(Some(binding)) = store.binding(&id) else {
        return HttpResponse::NotFound().body(format!("no session {id:?}"));
    };
    // Tidying power, not destruction: the page's ✕ deletes a *throwaway*
    // page's file, and merely unbinds anything committed — a promoted .sv
    // as much as an imported DESIGN.md (author, 2026-08-10). Deleting a
    // file someone committed belongs to git, or to an explicit
    // `page rm --file`, never to a two-click affordance in a browser.
    if crate::store::is_throwaway_page(&binding.path) {
        let file = state.root.join(&binding.path);
        if let Err(e) = std::fs::remove_file(&file) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return HttpResponse::InternalServerError()
                    .body(format!("removing {}: {e}", file.display()));
            }
        }
        let _ = std::fs::remove_file(file.with_extension("sv.lock"));
    }
    match store.delete_binding(&id) {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}

/// Re-find every page file the db doesn't know: committed .sv files bind by
/// stem (V2.sv → "V2"), throwaway pages under .sideview/pages/ by decoding
/// their filename back to the session id. Chip order for rediscovered pages
/// comes from canon: the `order` attribute on `<sv-page>` when the author
/// cares, path order otherwise — binding insertion order carries it.
fn rediscover_pages(store: &Store) -> Result<()> {
    let known: std::collections::HashSet<String> =
        store.bindings()?.into_iter().map(|b| b.path).collect();
    let mut found: Vec<(f64, String, String, String)> = Vec::new(); // (order, path, rel, id)

    let pages_dir = store.dir.join(crate::store::PAGES_DIR);
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
                    // The throwaway filename is the encoded session id.
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
        if store.binding(&id)?.is_some() {
            eprintln!("note: {rel} not rebound — a different file already holds page id {id:?}");
            continue;
        }
        store.bind_session(&id, &rel, &store.root.display().to_string(), "rediscovered")?;
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

    let attachments_root = state.store.lock().unwrap().dir.join(crate::store::ATTACHMENTS_DIR);
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
                "path": format!("{}{}/{}", crate::store::ATTACHMENTS_PREFIX, dir_name, name),
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
        "path": format!("{}{}/{}", crate::store::ATTACHMENTS_PREFIX, dir_name, name),
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
    #[serde(default)]
    attachments: Vec<crate::store::NewAttachment>,
}

async fn post_comment(body: web::Json<CommentBody>, state: Data<AppState>) -> impl Responder {
    let b = body.into_inner();
    if b.body.trim().is_empty() {
        return HttpResponse::BadRequest().body("empty comment body");
    }
    let mut store = state.store.lock().unwrap();
    // A row is a future deletion (page rm, gc), so verify each claimed
    // attachment is a real file in the attachments home before binding it.
    for a in &b.attachments {
        if !crate::store::is_attachment_path(&a.path) || !store.root.join(&a.path).is_file() {
            return HttpResponse::BadRequest()
                .body(format!("attachment {:?} is not an uploaded file", a.path));
        }
    }
    let result = match (b.thread, b.page.as_deref(), b.target.as_deref()) {
        (Some(tid), _, _) => {
            store.reply(tid, &b.body, Some("user"), &b.attachments).map(|cid| (tid, cid))
        }
        (None, Some(page), Some(target)) => store.create_thread(
            page,
            target,
            &b.anchor,
            b.quote.as_deref(),
            b.context.as_deref(),
            &b.body,
            Some("user"),
            &b.attachments,
        ),
        _ => return HttpResponse::BadRequest().body("pass thread, or page and target"),
    };
    match result {
        Ok((thread, id)) => HttpResponse::Ok().json(serde_json::json!({
            "thread": thread, "id": id,
        })),
        Err(e) => HttpResponse::BadRequest().body(format!("{e:#}")),
    }
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
    // Path segments arrive still percent-encoded (the /s/ route learned the
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
            crate::session::encode(&page_dec),
            crate::session::encode(&block_dec)
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
    match store.thread(id) {
        Ok(None) => HttpResponse::NotFound().body(format!("no thread {id}")),
        Ok(Some(_)) => match store.resolve_thread(id, Some("user"), undo) {
            Ok(_) => HttpResponse::NoContent().finish(),
            Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
        },
        Err(e) => HttpResponse::InternalServerError().body(format!("{e:#}")),
    }
}

/// One page's conversation, serialized for the `threads` SSE event. Sent
/// whole on every change — page-scale, same reasoning as block replay.
fn conversation_json(store: &Store, page: &str) -> Result<String> {
    // Comments travel rendered (body_html) beside their source: the card
    // shows comrak's safe-mode markdown, agents keep reading raw bodies.
    let comments: Vec<serde_json::Value> = store
        .comments_for_page(page)?
        .into_iter()
        .map(|c| {
            let mut v = serde_json::to_value(&c).expect("comment serializes");
            v["body_html"] = serde_json::Value::String(crate::render::comment_body(&c.body));
            v
        })
        .collect();
    Ok(serde_json::json!({
        "page": page,
        "threads": store.threads_for_page(page)?,
        "comments": comments,
        "attachments": store.attachments_for_page(page)?,
    })
    .to_string())
}

fn threads_event(data: String) -> Outgoing {
    Outgoing { kind: "threads", data }
}

/// Change detection, liveness, supersession and the deleted-underneath-us
/// check, all in one loop that is already running. Content change detection
/// is file stat, not `data_version`: the db no longer holds content.
fn poll_loop(
    store_dir: &Path,
    instance_id: &str,
    tx: &broadcast::Sender<Outgoing>,
    shared: &Arc<Mutex<Shared>>,
) -> Result<()> {
    let store = Store::open(store_dir)?;
    let db_path = store.db_path();
    let opened = std::fs::metadata(&db_path)?;
    let (dev, ino) = (opened.dev(), opened.ino());
    let mut ticks: u32 = 0;
    // Conversation change detection: the generation counter, bumped in the
    // same transaction as every conversation write — one O(1) row read.
    // (data_version was caught missing a cross-process commit under WAL
    // after a long idle, live 2026-08-08; an aggregate probe briefly stood
    // in before the author called for the counter.)
    let mut conversation_gen = -1i64;
    // The repo config is a file like any other: stat it on the same tick, so
    // a category or a new imported page lands live.
    let mut config_stamp: Option<Option<(SystemTime, u64)>> = None;
    let mut cfg = crate::config::Config::default();

    loop {
        ticks = ticks.wrapping_add(1);

        // `rm -rf .sideview/` under a live daemon: the open handle keeps the
        // inode alive and the daemon goes silently blind. Turn the silent
        // version into the noisy one.
        match std::fs::metadata(&db_path) {
            Ok(m) if m.dev() == dev && m.ino() == ino => {}
            _ => {
                eprintln!(
                    "store at {} was deleted or replaced underneath this daemon — exiting",
                    db_path.display()
                );
                std::process::exit(1);
            }
        }

        if ticks % HEARTBEAT_EVERY == 0 {
            // Zero rows affected means somebody else claimed the row: stop
            // serving and let the browser reconnect to whoever holds it now.
            if !store.heartbeat(instance_id)? {
                eprintln!("superseded by another daemon — exiting");
                std::process::exit(0);
            }
        }

        // Answer pings, only ever as the current holder.
        let pending: Option<(i64, i64)> = store
            .conn
            .query_row("SELECT ping, pong FROM daemon", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        if let Some((ping, pong)) = pending {
            if ping != pong {
                store.answer_ping(instance_id)?;
            }
        }

        // Config: reload on change, then make its pages exist. This is the
        // half that survives a deleted db — .sv files announce themselves in
        // the startup scan, a repo's markdown never does, and sideview must
        // not colonize it by scanning (V3.sv).
        let cfg_file = store.root.join(crate::config::FILE);
        let cfg_now = std::fs::metadata(&cfg_file)
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
        let config_changed = config_stamp != Some(cfg_now);
        if config_changed {
            config_stamp = Some(cfg_now);
            let (c, err) = crate::config::load(&store.root);
            if let Some(e) = err {
                eprintln!("config ignored — {e}");
            }
            cfg = c;
            let (exts, problems) = crate::config::load_extensions(&store.root, &cfg);
            for (path, why) in &problems {
                eprintln!("extension {path} not loaded — {why}");
            }
            shared.lock().unwrap().extensions = exts;
            for e in &cfg.pages {
                let id = e.page_id();
                if store.binding(&id)?.is_some() {
                    continue;
                }
                if !store.root.join(&e.path).exists() {
                    eprintln!("config: no file {} (page {id})", e.path);
                    continue;
                }
                store.bind_session(&id, &e.path, &store.root.display().to_string(), "config")?;
                eprintln!("config page {id} → {}", e.path);
            }
        }

        let mut changed_sessions = false;
        let mut events: Vec<Outgoing> = Vec::new();
        let bindings = store.bindings()?;
        {
            let mut shared = shared.lock().unwrap();

            let order: Vec<(String, i64)> =
                bindings.iter().map(|b| (b.id.clone(), b.last_active_at)).collect();
            if order != shared.sessions {
                shared.sessions = order;
                changed_sessions = true;
            }

            let exts = shared.extensions.clone();
            for b in &bindings {
                let file = store.root.join(&b.path);
                let stamp = std::fs::metadata(&file)
                    .ok()
                    .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
                let entry = cfg.pages.iter().find(|e| e.page_id() == b.id || e.path == b.path);
                let known = shared.pages.get(&b.id).map(|p| p.stamp);
                // A config edit can change how a page renders or what it is
                // called, so it re-reads even when the file itself is still —
                // and a changed *referenced* file (a csv a block points at)
                // re-renders even though the page file never moved.
                let refs_stale = shared
                    .pages
                    .get(&b.id)
                    .map(|p| {
                        p.file_refs.iter().any(|(rel, old)| file_stamp(&store.root.join(rel)) != *old)
                    })
                    .unwrap_or(false);
                if known == Some(stamp) && !config_changed && !refs_stale {
                    continue;
                }
                let fmt =
                    crate::config::format_of(&b.path, entry.and_then(|e| e.render.as_deref()));
                let mut fresh = load_page(&file, &b.path, stamp, fmt, &b.id, &exts, &store.root);
                // What the chip's ✕ may do, decided here where the tier and
                // the config are both in hand: a throwaway page is scratch
                // and closes by deleting; a committed one only unbinds; and
                // a page the config declares has no meaningful close at all,
                // since it returns the moment the config is read again.
                let tier = if crate::store::is_throwaway_page(&b.path) {
                    "throwaway"
                } else {
                    "committed"
                };
                fresh.props.insert("tier".into(), serde_json::Value::String(tier.into()));
                // The index shows where a page lives; the file is the page.
                fresh.props.insert("path".into(), serde_json::Value::String(b.path.clone()));
                fresh.props.insert(
                    "closable".into(),
                    serde_json::Value::String(
                        if entry.is_some() { "config" } else { "yes" }.into(),
                    ),
                );
                // Config supplies only what the file cannot say about itself:
                // an .sv page's own props always win.
                if let Some(e) = entry {
                    for (key, val) in [
                        ("label", e.label.clone()),
                        ("category", e.category.clone()),
                        ("order", e.order.map(|o| o.to_string())),
                    ] {
                        if let Some(v) = val {
                            fresh.props.entry(key.to_string()).or_insert(serde_json::Value::String(v));
                        }
                    }
                }
                let old = shared.pages.insert(b.id.clone(), fresh);
                let fresh = &shared.pages[&b.id];
                if old.as_ref().map(|o| &o.props) != Some(&fresh.props) {
                    changed_sessions = true;
                }
                events.extend(diff_events(&b.id, old.as_ref(), fresh));
            }
            // A binding that vanished takes its page state with it. (Nothing
            // deletes bindings in this slice, but be tolerant of a reset.)
            let live: std::collections::HashSet<&String> =
                bindings.iter().map(|b| &b.id).collect();
            shared.pages.retain(|id, _| live.contains(id));

            // On any conversation mutation: re-serialize conversation
            // snapshots (shipping the pages that changed), and reload the
            // explicit outlines, which ride the sessions event as a prop.
            let g = store.conversation_gen().unwrap_or(conversation_gen);
            if g != conversation_gen {
                conversation_gen = g;

                let fresh: HashMap<String, serde_json::Value> = store
                    .outlines()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(page, spec)| {
                        serde_json::from_str(&spec).ok().map(|v| (page, v))
                    })
                    .collect();
                if fresh != shared.outlines {
                    shared.outlines = fresh;
                    changed_sessions = true;
                }

                let pages = store.conversation_pages().unwrap_or_default();
                for page in &pages {
                    if let Ok(json) = conversation_json(&store, page) {
                        if shared.conversations.get(page) != Some(&json) {
                            shared.conversations.insert(page.clone(), json.clone());
                            events.push(threads_event(json));
                        }
                    }
                }
                let conversing: std::collections::HashSet<&String> = pages.iter().collect();
                shared.conversations.retain(|page, _| {
                    let keep = conversing.contains(page);
                    if !keep {
                        events.push(threads_event(
                            serde_json::json!({
                                "page": page, "threads": [], "comments": [], "attachments": [],
                            })
                            .to_string(),
                        ));
                    }
                    keep
                });
            }

            if changed_sessions {
                events.insert(0, sessions_event(&shared));
            }
        }
        for e in events {
            let _ = tx.send(e);
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Read and render a page file into daemon state. A missing or unreadable
/// file renders as one honest block rather than an empty page — a binding
/// pointing at nothing is a fact worth showing.
/// An imported file as a one-block page. One block and not one per heading,
/// deliberately: block ids are what comments target, so heading-derived ids
/// would orphan every thread under a renamed heading (V3.sv).
fn imported_page(source: &str, fmt: crate::config::Format) -> format::Page {
    use crate::config::Format;
    let type_name = match fmt {
        Format::Markdown => "sv-prose",
        Format::HtmlInline => "sv-markup",
        Format::HtmlFrame => "sv-html",
        Format::Sv => unreachable!("sv files are parsed, not imported"),
    };
    format::Page {
        props: Vec::new(), // label/category come from config: the file cannot say
        blocks: vec![format::Block {
            type_name: type_name.into(),
            // A stable id, so a doc's comments survive every edit to it: the
            // anchor inside does the finer work.
            attrs: vec![("id".into(), "doc".into())],
            body: source.to_string(),
            warnings: Vec::new(),
            lines: (0, 0), // never spliced: the CLI only edits .sv pages
        }],
        warnings: Vec::new(),
    }
}

/// (mtime, len) of a file, None when it is missing — the shared shape for
/// page stamps and referenced-file stamps.
fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())))
}

fn load_page(
    file: &Path,
    rel: &str,
    stamp: Option<(SystemTime, u64)>,
    fmt: crate::config::Format,
    page_id: &str,
    exts: &[crate::config::Extension],
    root: &Path,
) -> PageState {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            let msg = format!("page file moved or deleted — was {rel}");
            return PageState {
                ext_blocks: HashMap::new(),
                file_refs: Vec::new(),
                props: serde_json::Map::new(),
                blocks: vec![Rendered {
                    id: "sv-missing".into(),
                    ord: "a000000000000".into(),
                    html: format!(
                        r#"<section class="sv-block sv-degraded" data-block="sv-missing" data-type="sv-missing"><p class="sv-degraded-note">{}</p></section>"#,
                        msg.replace('<', "&lt;")
                    ),
                    headings_json: serde_json::json!([]),
                }],
                stamp,
            }
        }
    };
    // Three page formats, one renderer (V3.sv). An imported file is a page
    // sideview did not compose, so it becomes exactly one block through the
    // path that already exists for its content — no new block type, and the
    // limitations belong to the format.
    let page = match fmt {
        crate::config::Format::Sv => format::parse(&source),
        other => imported_page(&source, other),
    };
    let mut props = serde_json::Map::new();
    for (k, v) in &page.props {
        props.insert(k.clone(), serde_json::Value::String(v.clone()));
    }

    // Effective ids: the id attribute, or a content hash for blocks without
    // one (stable across reorders; an edit reads as remove-and-add, which is
    // the accepted cost of not naming a block). Duplicate hashes — two
    // identical anonymous blocks — get an occurrence suffix.
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut blocks = Vec::new();
    let mut ext_blocks: HashMap<String, (Vec<(String, String)>, String)> = HashMap::new();
    let mut file_refs: Vec<(String, Option<(SystemTime, u64)>)> = Vec::new();
    for (i, b) in page.blocks.iter().enumerate() {
        let base = match b.id() {
            Some(id) => id.to_string(),
            None => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (&b.type_name, &b.body).hash(&mut h);
                format!("h{:012x}", h.finish() & 0xffff_ffff_ffff)
            }
        };
        let n = seen.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = if *n == 1 { base } else { format!("{base}-{n}") };
        // sv-csv reads its referenced file here, where root and the poll
        // loop live: content confined to the project (the /f/ rule), the
        // stamp recorded so an overwritten csv re-renders the block.
        if b.type_name == "sv-csv" {
            let src = b.attr("src").unwrap_or("").trim().to_string();
            let content = if src.is_empty() {
                Err("sv-csv needs src=\"<project-relative .csv>\"".to_string())
            } else if src.starts_with('/') || src.split('/').any(|seg| seg == "..") {
                Err(format!("{src}: outside the project"))
            } else {
                let abs = root.join(&src);
                file_refs.push((src.clone(), file_stamp(&abs)));
                std::fs::read_to_string(&abs)
                    .map_err(|e| format!("{src}: {e} — the block heals when the file appears"))
            };
            blocks.push(Rendered {
                ord: format!("a{i:012}"),
                html: crate::csv::block(&id, b, content),
                headings_json: serde_json::json!([]),
                id,
            });
            continue;
        }
        // A tag a registered extension claims renders as that extension's
        // frame; everything else takes the normal path (which ends at the
        // honest unknown-tag block).
        if let Some(ext) = exts.iter().find(|x| x.tag() == b.type_name) {
            ext_blocks.insert(id.clone(), (b.attrs.clone(), b.body.clone()));
            blocks.push(Rendered {
                ord: format!("a{i:012}"),
                html: render::ext_block(&id, &ext.manifest.name, page_id, b.attr("height")),
                headings_json: serde_json::json!([]),
                id,
            });
            continue;
        }
        blocks.push(Rendered {
            ord: format!("a{i:012}"),
            html: render::block(&id, b),
            headings_json: serde_json::to_value(render::outline(&id, b))
                .unwrap_or_else(|_| serde_json::json!([])),
            id,
        });
    }
    PageState { props, blocks, stamp, ext_blocks, file_refs }
}

/// The reparse diff: upserts for new or changed blocks, removes for gone ones.
/// This is what keeps live patching surgical even though the source of truth
/// is a whole file.
fn diff_events(session: &str, old: Option<&PageState>, new: &PageState) -> Vec<Outgoing> {
    let mut events = Vec::new();
    let old_by_id: HashMap<&str, &Rendered> = old
        .map(|o| o.blocks.iter().map(|b| (b.id.as_str(), b)).collect())
        .unwrap_or_default();
    for b in &new.blocks {
        if old_by_id.get(b.id.as_str()) != Some(&&*b) {
            events.push(block_event(session, b));
        }
    }
    let new_ids: std::collections::HashSet<&str> =
        new.blocks.iter().map(|b| b.id.as_str()).collect();
    for b in old.map(|o| o.blocks.as_slice()).unwrap_or_default() {
        if !new_ids.contains(b.id.as_str()) {
            events.push(Outgoing {
                kind: "block",
                data: serde_json::json!({
                    "session": session,
                    "block": b.id,
                    "action": "remove",
                })
                .to_string(),
            });
        }
    }
    events
}

fn block_event(session: &str, b: &Rendered) -> Outgoing {
    Outgoing {
        kind: "block",
        data: serde_json::json!({
            "session": session,
            "block": b.id,
            "action": "upsert",
            "ord": b.ord,
            "html": b.html,
            "headings": b.headings_json,
        })
        .to_string(),
    }
}

fn sessions_event(shared: &Shared) -> Outgoing {
    let sessions: Vec<serde_json::Value> = shared
        .sessions
        .iter()
        .map(|(id, last_active_at)| {
            // Props pass through whole from the file, so a key a newer CLI
            // writes reaches the page even through a daemon that has never
            // heard of it. An explicit outline (db, not file) rides along
            // as outline_spec — used verbatim by the rail when present.
            let mut props = shared
                .pages
                .get(id)
                .map(|p| p.props.clone())
                .unwrap_or_default();
            if let Some(spec) = shared.outlines.get(id) {
                props.insert("outline_spec".into(), spec.clone());
            }
            serde_json::json!({ "id": id, "last_active_at": last_active_at, "props": props })
        })
        .collect();
    Outgoing {
        kind: "sessions",
        data: serde_json::json!({ "sessions": sessions }).to_string(),
    }
}

fn to_sse(o: Outgoing) -> sse::Event {
    sse::Event::Data(sse::Data::new(o.data).event(o.kind))
}

/// `/` — the server owns routing now (author, 2026-08-18): redirect to the
/// most recently active page, which is what the client's auto-follow used
/// to decide and the URL now simply *is*. An empty project gets the shell,
/// which renders its honest "no pages yet".
async fn root_redirect(state: Data<AppState>) -> HttpResponse {
    let most_active = {
        let shared = state.shared.lock().unwrap();
        shared
            .sessions
            .iter()
            .max_by_key(|(_, at)| *at)
            .map(|(id, _)| id.clone())
    };
    match most_active {
        Some(id) => HttpResponse::Found()
            .insert_header(("Location", format!("/s/{}", crate::session::encode(&id))))
            // The choice changes as pages become active: never cache it.
            .insert_header(("Cache-Control", "no-store"))
            .finish(),
        None => page().await,
    }
}

async fn page() -> HttpResponse {
    match Assets::get("index.html") {
        Some(f) => {
            // no-cache asks politely; iOS sometimes pairs a fresh script with
            // stale CSS anyway (bit live twice — HANDOFF's mobile saga, then
            // thread 35). A per-daemon-start stamp on the asset URLs makes
            // every restart a hard bust, and a restart is already how a new
            // binary arrives.
            static STAMP: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
            let v = STAMP.get_or_init(crate::store::now_ms);
            let html = String::from_utf8_lossy(&f.data)
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
        replay.push(to_sse(sessions_event(&shared)));
        for (id, _) in &shared.sessions {
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
    if let Ok(store_dir) = root.join(crate::store::DIR_NAME).canonicalize() {
        if full.starts_with(&store_dir) {
            let name = full.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with(crate::store::DB_FILE)
                || name == "daemon.log"
                || name == crate::store::SPAWN_LOCK
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

    fn page_from(src: &str) -> PageState {
        let dir = std::env::temp_dir().join(format!("sv-pg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.sv");
        std::fs::write(&file, src).unwrap();
        load_page(&file, "p.sv", None, crate::config::Format::Sv, "test", &[], &dir)
    }

    #[test]
    fn reparse_diff_is_surgical() {
        let old = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-prose id=\"b2\">\ntwo\n</sv-prose>\n");
        let new = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-prose id=\"b2\">\ntwo, revised\n</sv-prose>\n");
        let events = diff_events("s", Some(&old), &new);
        assert_eq!(events.len(), 1, "only the changed block goes out: {events:?}");
        assert!(events[0].data.contains("b2"));
        assert!(events[0].data.contains("revised"));
    }

    #[test]
    fn removed_blocks_emit_removes_and_anonymous_blocks_get_stable_ids() {
        let old = page_from("<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-markup>\n<b>anon</b>\n</sv-markup>\n");
        let new = page_from("<sv-markup>\n<b>anon</b>\n</sv-markup>\n");
        // b1 gone; the anonymous block keeps its hash id across the reorder,
        // so the only event is the remove.
        let events = diff_events("s", Some(&old), &new);
        let removes: Vec<_> = events.iter().filter(|e| e.data.contains("remove")).collect();
        assert_eq!(removes.len(), 1, "{events:?}");
        assert!(removes[0].data.contains("b1"));
        let upserts: Vec<_> = events.iter().filter(|e| e.data.contains("upsert")).collect();
        assert_eq!(upserts.len(), 1, "the anon block moved position (ord changed): {events:?}");
    }

    #[test]
    fn a_missing_file_renders_an_honest_block() {
        let page = load_page(Path::new("/nonexistent/nowhere.sv"), "nowhere.sv", None, crate::config::Format::Sv, "test", &[], Path::new("/tmp"));
        assert_eq!(page.blocks.len(), 1);
        assert!(page.blocks[0].html.contains("moved or deleted"), "{}", page.blocks[0].html);
    }

    #[actix_web::test]
    async fn file_endpoint_refuses_store_internals_but_serves_neighbours() {
        let dir = std::env::temp_dir().join(format!("sv-fe-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        std::fs::write(store.root.join("ok.html"), "<p>ok</p>").unwrap();
        std::fs::write(store.dir.join("lab.html"), "<p>lab</p>").unwrap();
        std::fs::write(store.dir.join("sideview.db-pre-v4"), "backup").unwrap();
        std::fs::write(store.dir.join(crate::store::SPAWN_LOCK), "").unwrap();
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
    async fn delete_session_removes_file_and_binding_and_404s_on_unknown() {
        let dir = std::env::temp_dir().join(format!("sv-del-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        store.bind_session("s1", ".sideview/pages/s1.sv", "/tmp", "test").unwrap();
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
                .route("/api/sessions/{session}", web::delete().to(delete_session)),
        )
        .await;

        let req = actix_web::test::TestRequest::delete().uri("/api/sessions/nope").to_request();
        let res = actix_web::test::call_service(&app, req).await;
        assert_eq!(res.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = actix_web::test::TestRequest::delete().uri("/api/sessions/s1").to_request();
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
        state.store.lock().unwrap().bind_session("DESIGN", "DESIGN.md", "/tmp", "config").unwrap();
        let req = actix_web::test::TestRequest::delete().uri("/api/sessions/DESIGN").to_request();
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
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
        // Two committed pages, order attribute inverting path order…
        std::fs::write(store.root.join("zebra.sv"), "<sv-page order=\"1\">\n</sv-page>\n").unwrap();
        std::fs::write(store.root.join("alpha.sv"), "<sv-page order=\"2\">\n</sv-page>\n").unwrap();
        // …and a throwaway whose filename encodes its session id.
        std::fs::write(
            store.pages_dir().unwrap().join("cwd%3A%2Ftmp%2Fp.sv"),
            "<sv-prose id=\"b1\">\nx\n</sv-prose>\n",
        )
        .unwrap();
        rediscover_pages(&store).unwrap();
        let bindings = store.bindings().unwrap();
        let ids: Vec<&str> = bindings.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["zebra", "alpha", "cwd:/tmp/p"],
            "order attr beats path order; throwaway ids decode"
        );
        // Idempotent: a second scan binds nothing new.
        rediscover_pages(&store).unwrap();
        assert_eq!(store.bindings().unwrap().len(), 3);
    }

    #[actix_web::test]
    async fn comment_endpoint_creates_threads_replies_and_resolves() {
        let dir = std::env::temp_dir().join(format!("sv-cm-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
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
        assert_eq!(store.comments_for_page("v2").unwrap().len(), 2);
        assert!(store.threads_for_page("v2").unwrap()[0].resolved_at.is_none());
    }

    #[actix_web::test]
    async fn attachments_upload_dedupe_bind_and_refuse_impostors() {
        let dir = std::env::temp_dir().join(format!("sv-at-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
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
        assert!(rel.starts_with(crate::store::ATTACHMENTS_PREFIX));
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
            let atts = store.attachments_for_page("v3").unwrap();
            assert_eq!(atts.len(), 1);
            assert_eq!(atts[0].path, rel);
            let json = conversation_json(&store, "v3").unwrap();
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
        let store = Store::open(&dir.join(crate::store::DIR_NAME)).unwrap();
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
        let mut page = PageState::default();
        page.ext_blocks.insert(
            "b1".into(),
            (vec![("db".into(), "x.duckdb".into())], "select 1".into()),
        );
        let shared = Shared {
            extensions: vec![ext],
            pages: HashMap::from([("v3".to_string(), page)]),
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

    /// Session ids can contain `/` and `%` (cwd and tmux rungs). The printed
    /// URLs percent-encode them; this pins that the encoded form actually
    /// matches the single-segment page route.
    #[actix_web::test]
    async fn encoded_session_ids_match_the_page_route() {
        let app = actix_web::test::init_service(
            actix_web::App::new().route("/s/{session}", actix_web::web::get().to(page)),
        )
        .await;
        for uri in ["/s/cwd%3A%2Fhome%2Fdavid%2Fproj", "/s/tmux%2542"] {
            let req = actix_web::test::TestRequest::get().uri(uri).to_request();
            let res = actix_web::test::call_service(&app, req).await;
            assert!(res.status().is_success(), "{uri} did not match the page route");
        }
    }
}

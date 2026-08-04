//! The daemon: serves the page, notices new blocks by polling
//! `PRAGMA data_version`, patches the open page over SSE. Long-lived, no idle
//! exit (auto-exit depends on auto-restart, which is precisely what a
//! sandboxed agent cannot do).

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use actix_web::web::{self, Data};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_lab::sse;
use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::netcheck;
use crate::render;
use crate::store::{now_ms, BlockRow, DaemonRow, Store};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEARTBEAT_EVERY: u32 = 12; // × POLL_INTERVAL ≈ 3s

#[derive(Debug, Clone)]
pub struct Opts {
    /// `--bind auto`: loopback plus the tailnet address when there is one.
    pub bind_auto: bool,
    /// Open the browser once the row is claimed (the auto-spawn "first use"
    /// path; bare foreground `sideview` also uses it).
    pub open_browser: bool,
}

/// A pre-serialized SSE event, fanned out to every connected page.
#[derive(Debug, Clone)]
struct Outgoing {
    kind: &'static str, // "block" | "sessions"
    id: Option<i64>,    // the rev, for block events
    data: String,
}

struct AppState {
    store: Mutex<Store>,
    root: PathBuf,
    tx: broadcast::Sender<Outgoing>,
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

    // Port: remembered so an open tab reconnects to the same origin across
    // restarts. Taken? Take another and print the new URL. Meta is the
    // durable copy (the row is cleared on clean shutdown); a crash-leftover
    // row is the fallback.
    let remembered = store
        .meta("port")?
        .and_then(|p| p.parse().ok())
        .or(store.daemon()?.map(|d| d.port))
        .unwrap_or(0);
    let loopback = TcpListener::bind((Ipv4Addr::LOCALHOST, remembered))
        .or_else(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
        .context("binding loopback")?;
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

    // The poll loop gets its own connection on its own thread; handlers share
    // the claiming connection behind a mutex.
    {
        let dir = store_dir.to_path_buf();
        let tx = tx.clone();
        let instance_id = instance_id.clone();
        std::thread::spawn(move || {
            if let Err(e) = poll_loop(&dir, &instance_id, &tx) {
                eprintln!("poll loop died: {e:#}");
                std::process::exit(1);
            }
        });
    }

    let root = store.root.clone();
    let state = Data::new(AppState { store: Mutex::new(store), root, tx });

    let server_state = state.clone();
    actix_web::rt::System::new().block_on(async move {
        let mut server = HttpServer::new(move || {
            App::new()
                .app_data(server_state.clone())
                .route("/", web::get().to(page))
                .route("/s/{session}", web::get().to(page))
                .route("/events", web::get().to(events))
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

/// Change detection, liveness, supersession and the deleted-underneath-us
/// check, all in one loop that is already running.
fn poll_loop(store_dir: &Path, instance_id: &str, tx: &broadcast::Sender<Outgoing>) -> Result<()> {
    let store = Store::open(store_dir)?;
    let db_path = store.db_path();
    let opened = std::fs::metadata(&db_path)?;
    let (dev, ino) = (opened.dev(), opened.ino());

    let mut last_rev = store.max_rev()?;
    let mut data_version: i64 =
        store.conn.pragma_query_value(None, "data_version", |r| r.get(0))?;
    let mut ticks: u32 = 0;

    loop {
        std::thread::sleep(POLL_INTERVAL);
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

        // data_version only moves on other connections' commits, which is
        // exactly the signal wanted.
        let dv: i64 = store.conn.pragma_query_value(None, "data_version", |r| r.get(0))?;
        if dv == data_version {
            continue;
        }
        data_version = dv;

        for b in store.blocks_since(last_rev)? {
            last_rev = b.rev.max(last_rev);
            let _ = tx.send(block_event(&b));
        }
        let _ = tx.send(sessions_event(&store)?);
    }
}

fn block_event(b: &BlockRow) -> Outgoing {
    let (action, html, headings) = if b.deleted {
        ("remove", String::new(), Vec::new())
    } else {
        (
            "upsert",
            render::block(&b.short_id, &b.spec_json),
            render::outline(&b.short_id, &b.spec_json),
        )
    };
    Outgoing {
        kind: "block",
        id: Some(b.rev),
        data: serde_json::json!({
            "session": b.session_id,
            "block": b.short_id,
            "action": action,
            "ord": b.ord,
            "html": html,
            "headings": headings,
        })
        .to_string(),
    }
}

fn sessions_event(store: &Store) -> Result<Outgoing> {
    let sessions: Vec<serde_json::Value> = store
        .sessions()?
        .into_iter()
        // Props pass through whole, so a preference a newer CLI wrote reaches
        // the page even through a daemon that has never heard of it.
        .map(|s| {
            serde_json::json!({
                "id": s.id, "last_active_at": s.last_active_at, "props": s.props,
            })
        })
        .collect();
    Ok(Outgoing {
        kind: "sessions",
        id: None,
        data: serde_json::json!({ "sessions": sessions }).to_string(),
    })
}

fn to_sse(o: Outgoing) -> sse::Event {
    let mut data = sse::Data::new(o.data).event(o.kind);
    if let Some(id) = o.id {
        data = data.id(id.to_string());
    }
    sse::Event::Data(data)
}

async fn page() -> impl Responder {
    match Assets::get("index.html") {
        Some(f) => HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(f.data.into_owned()),
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
            .body(f.data.into_owned()),
        None => HttpResponse::NotFound().body(format!("no embedded asset {rel}")),
    }
}

/// One long-lived stream per page. Reconnection is `Last-Event-ID` replay —
/// `where rev > N` — so a laptop that slept for an hour reattaches and gets
/// what it missed, tombstones included.
async fn events(req: HttpRequest, state: Data<AppState>) -> actix_web::Result<impl Responder> {
    let last: i64 = req
        .headers()
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Subscribe before querying: an event landing in between is delivered
    // twice, and upserts are idempotent; the other order loses it.
    let rx = state.tx.subscribe();

    let mut replay: Vec<sse::Event> = Vec::new();
    {
        let store = state.store.lock().unwrap();
        replay.push(to_sse(sessions_event(&store).map_err(err500)?));
        for b in store.blocks_since(last).map_err(err500)? {
            // A fresh page doesn't need tombstones of blocks it never saw.
            if last == 0 && b.deleted {
                continue;
            }
            replay.push(to_sse(block_event(&b)));
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
    // The store's internals are not project content — the db holds every
    // session's blocks and is otherwise readable by any tailnet node. Only
    // the named internals are refused: other files under .sideview/ still
    // serve (the dogfood comparison pages iframe from there).
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

fn err500(e: anyhow::Error) -> actix_web::Error {
    actix_web::error::ErrorInternalServerError(format!("{e:#}"))
}

/// The live half of an SSE stream. A subscriber that falls behind the
/// broadcast buffer gets `Err(Lagged)` — it has *lost events*, so the stream
/// must end rather than resume: a dropped stream makes `EventSource`
/// reconnect with `Last-Event-ID`, and the replay heals the gap. Skipping the
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
    use futures_util::StreamExt as _;

    #[test]
    fn lagged_subscriber_stream_ends_instead_of_resuming() {
        actix_web::rt::System::new().block_on(async {
            let (tx, rx) = broadcast::channel::<Outgoing>(1);
            // Three sends into a one-slot buffer: the receiver has lost events.
            for i in 0..3 {
                tx.send(Outgoing { kind: "block", id: Some(i), data: String::new() }).unwrap();
            }
            drop(tx);
            let got: Vec<_> = live_events(rx).collect().await;
            assert_eq!(
                got.len(),
                0,
                "a lagged stream must terminate (forcing replay), not skip ahead"
            );
        });
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
        let state = Data::new(AppState { root: store.root.clone(), store: Mutex::new(store), tx });
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/f/{path:.*}", web::get().to(project_file)),
        )
        .await;
        for (path, expect_ok) in [
            ("/f/ok.html", true),
            ("/f/.sideview/lab.html", true),                // labs iframe from here
            ("/f/.sideview/sideview.db", false),            // every session's blocks
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

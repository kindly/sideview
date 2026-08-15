//! The extension mechanism's working parts (EXTENSIONS.md is the contract):
//! serving an extension's entry with the injections, and running its one
//! declared binary — per call, fresh process, argv array, no shell. The
//! daemon owns the routes; everything testable without HTTP lives here.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

use crate::config::Extension;

/// Per-call ceilings, mechanism-enforced (EXTENSIONS.md: "not yours to opt
/// out of"). A runaway child must not hang a handler or push half a
/// gigabyte into a page.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
pub const OUTPUT_CAP: usize = 64 * 1024 * 1024;

/// What `call_cli` resolves to in the frame.
#[derive(Debug, serde::Serialize)]
pub struct CallResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct CallBody {
    pub args: Vec<String>,
    #[serde(default)]
    pub stdin: Option<String>,
}

/// The manifest's `bin`, resolved: "./…" is relative to the extension's
/// directory, a bare name is left for PATH. Never anything the *caller*
/// chose — the frame picks arguments, the manifest picks the executable.
pub fn resolve_bin(root: &Path, ext: &Extension) -> Result<PathBuf> {
    let bin = ext
        .manifest
        .bin
        .as_deref()
        .with_context(|| format!("extension {:?} declares no bin", ext.manifest.name))?;
    Ok(if let Some(rel) = bin.strip_prefix("./") {
        root.join(&ext.dir).join(rel)
    } else {
        PathBuf::from(bin)
    })
}

fn command(bin: &Path, args: &[String], root: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .current_dir(root) // paths in args are project-relative (the contract)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The kill half of "aborting the request kills the child": dropping
        // the handle — stream cancelled, timeout hit — takes the child down.
        .kill_on_drop(true);
    cmd
}

/// One fresh process, stdin written, both pipes drained concurrently under
/// the caps. A non-zero exit is a *result*, not an error — the frame reads
/// `code`; errors here are the mechanism's own failures (spawn, timeout,
/// cap), which the handler turns into non-2xx responses.
pub async fn run_call(
    bin: &Path,
    args: &[String],
    stdin: Option<&str>,
    root: &Path,
) -> Result<CallResult> {
    let mut child = command(bin, args, root)
        .spawn()
        .with_context(|| format!("spawning {}", bin.display()))?;

    if let Some(data) = stdin {
        let mut handle = child.stdin.take().context("child stdin")?;
        // A child may exit without reading; a broken pipe is its answer,
        // not our failure.
        let _ = handle.write_all(data.as_bytes()).await;
    } else {
        drop(child.stdin.take());
    }

    let mut out_pipe = child.stdout.take().context("child stdout")?;
    let mut err_pipe = child.stderr.take().context("child stderr")?;
    let work = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        // Concurrent drains, or a child filling one pipe deadlocks the other.
        let mut out_take = (&mut out_pipe).take((OUTPUT_CAP + 1) as u64);
        let mut err_take = (&mut err_pipe).take((OUTPUT_CAP + 1) as u64);
        let (o, e) = tokio::join!(
            out_take.read_to_end(&mut stdout),
            err_take.read_to_end(&mut stderr),
        );
        o?;
        e?;
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>((status, stdout, stderr))
    };
    let (status, stdout, stderr) = tokio::time::timeout(CALL_TIMEOUT, work)
        .await
        .map_err(|_| anyhow::anyhow!("timed out after {}s", CALL_TIMEOUT.as_secs()))??;

    if stdout.len() > OUTPUT_CAP || stderr.len() > OUTPUT_CAP {
        bail!("output cap ({} MB) exceeded", OUTPUT_CAP / (1024 * 1024));
    }
    Ok(CallResult {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// The streaming variant: spawn, write stdin, hand back a stream of stdout
/// chunks as the child produces them. The child travels *inside* the stream
/// state, so the client aborting the fetch drops the stream, which drops
/// the child, which `kill_on_drop`s it — cancellation with no bookkeeping.
/// stderr drains to the daemon's log (a failure after the first byte can
/// only truncate; EXTENSIONS.md tells extensions to end-mark their formats).
pub fn stream_call(
    bin: PathBuf,
    args: Vec<String>,
    stdin: Option<String>,
    root: PathBuf,
    ext_name: String,
) -> Result<impl futures_util::Stream<Item = Result<actix_web::web::Bytes, std::io::Error>>> {
    let mut child = command(&bin, &args, &root)
        .spawn()
        .with_context(|| format!("spawning {}", bin.display()))?;

    let mut stdin_handle = child.stdin.take();
    let stdout = child.stdout.take().context("child stdout")?;
    let stderr = child.stderr.take().context("child stderr")?;

    // stdin and stderr each get their own task; the stream owns the child.
    actix_web::rt::spawn(async move {
        if let (Some(handle), Some(data)) = (stdin_handle.as_mut(), stdin) {
            let _ = handle.write_all(data.as_bytes()).await;
        }
        drop(stdin_handle);
    });
    actix_web::rt::spawn(async move {
        let mut err = String::new();
        let mut pipe = stderr;
        let _ = pipe.take(64 * 1024).read_to_string(&mut err).await;
        if !err.trim().is_empty() {
            eprintln!("extension {ext_name} stderr: {}", err.trim());
        }
    });

    let deadline = tokio::time::Instant::now() + CALL_TIMEOUT;
    Ok(futures_util::stream::unfold(
        (child, stdout, 0usize),
        move |(child, mut stdout, sent)| async move {
            let mut buf = vec![0u8; 64 * 1024];
            let read = tokio::time::timeout_at(deadline, stdout.read(&mut buf)).await;
            match read {
                Ok(Ok(0)) => None, // clean end; child reaped by kill_on_drop/wait
                Ok(Ok(n)) => {
                    if sent + n > OUTPUT_CAP {
                        eprintln!("extension stream truncated at output cap");
                        return None;
                    }
                    buf.truncate(n);
                    Some((Ok(actix_web::web::Bytes::from(buf)), (child, stdout, sent + n)))
                }
                Ok(Err(e)) => Some((Err(e), (child, stdout, sent))),
                Err(_) => {
                    eprintln!("extension stream timed out after {}s", CALL_TIMEOUT.as_secs());
                    None // dropping the state kills the child
                }
            }
        },
    ))
}

/// The serve-time injections (EXTENSIONS.md "What sideview injects"): the
/// opaque `<base>`, the block context, the two-function API, and live theme
/// mirroring — placed immediately after `<head>` so the base precedes every
/// resource reference, or prepended when the entry has no head.
pub fn inject_entry(entry_html: &str, base: &str, block_json: &str) -> String {
    let prelude = format!(
        concat!(
            r#"<base href="{base}">"#,
            "<script>\n",
            "window.SIDEVIEW_BLOCK = {block};\n",
            "window.sideview = {{\n",
            "  async call_cli(args, opts = {{}}) {{\n",
            "    const r = await fetch('__call', {{ method: 'POST',\n",
            "      headers: {{ 'Content-Type': 'application/json' }},\n",
            "      body: JSON.stringify({{ args, stdin: opts.stdin ?? null }}),\n",
            "      signal: opts.signal }});\n",
            "    if (!r.ok) throw new Error(await r.text());\n",
            "    return r.json();\n",
            "  }},\n",
            "  call_cli_streaming(args, opts = {{}}) {{\n",
            "    return fetch('__call_stream', {{ method: 'POST',\n",
            "      headers: {{ 'Content-Type': 'application/json' }},\n",
            "      body: JSON.stringify({{ args, stdin: opts.stdin ?? null }}),\n",
            "      signal: opts.signal }});\n",
            "  }},\n",
            "}};\n",
            // Same-origin theme mirror: read the page's data-bs-theme and
            // follow it live. try/catch so a foreign embed of the entry
            // (opened directly, no parent) still renders.
            "(function () {{ try {{\n",
            "  var p = parent.document.documentElement;\n",
            "  var sync = function () {{ document.documentElement.setAttribute(\n",
            "    'data-bs-theme', p.getAttribute('data-bs-theme') || 'light'); }};\n",
            "  new MutationObserver(sync).observe(p, {{ attributes: true, attributeFilter: ['data-bs-theme'] }});\n",
            "  sync();\n",
            "}} catch (e) {{}} }})();\n",
            "</script>",
        ),
        base = base,
        block = block_json,
    );
    let lower = entry_html.to_ascii_lowercase();
    if let Some(head) = lower.find("<head") {
        if let Some(close) = entry_html[head..].find('>') {
            let at = head + close + 1;
            return format!("{}{}{}", &entry_html[..at], prelude, &entry_html[at..]);
        }
    }
    format!("{prelude}{entry_html}")
}

/// SIDEVIEW_BLOCK as a script-safe JSON literal: serde makes it JSON, and
/// escaping `</` keeps a body containing "</script>" from ending the
/// injected tag early.
pub fn block_json(page: &str, id: &str, attrs: &[(String, String)], body: &str) -> String {
    let attrs_map: serde_json::Map<String, serde_json::Value> = attrs
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    serde_json::json!({ "page": page, "id": id, "attrs": attrs_map, "body": body })
        .to_string()
        .replace("</", "<\\/")
}

/// Static-file confinement, the same rule as `/f/`: resolve, then refuse
/// anything that escapes the extension's own directory.
pub fn safe_ext_file(root: &Path, ext: &Extension, tail: &str) -> Option<PathBuf> {
    let dir = root.join(&ext.dir).canonicalize().ok()?;
    let path = dir.join(tail).canonicalize().ok()?;
    path.starts_with(&dir).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Extension, Manifest};

    fn ext(bin: Option<&str>) -> Extension {
        Extension {
            manifest: Manifest {
                name: "demo".into(),
                api: 1,
                render: "frame".into(),
                entry: "index.html".into(),
                bin: bin.map(str::to_string),
            },
            dir: "extensions/demo".into(),
        }
    }

    #[test]
    fn bin_resolution_is_the_manifests_choice() {
        let root = Path::new("/proj");
        assert_eq!(resolve_bin(root, &ext(Some("wc"))).unwrap(), PathBuf::from("wc"));
        assert_eq!(
            resolve_bin(root, &ext(Some("./tool"))).unwrap(),
            PathBuf::from("/proj/extensions/demo/tool")
        );
        assert!(resolve_bin(root, &ext(None)).is_err(), "no bin declared, no call");
    }

    #[test]
    fn injection_precedes_the_entrys_own_scripts_and_survives_script_bodies() {
        let entry = "<!doctype html><head><title>x</title></head><body>\
                     <script type=\"module\">import 'vue'</script></body>";
        let json = block_json("V3", "b1", &[("db".into(), "x.duckdb".into())], "a </script> body");
        let out = inject_entry(entry, "/x/demo/V3/b1/", &json);
        let base = out.find("<base href=\"/x/demo/V3/b1/\">").expect("base injected");
        let api = out.find("window.sideview").expect("api injected");
        let own = out.find("type=\"module\"").expect("entry's own script kept");
        assert!(base < api && api < own, "base, then api, then the entry's scripts");
        assert!(out.contains("<\\/script>"), "a body's </script> cannot end the tag");
        // Headless entries still get the prelude, prepended.
        assert!(inject_entry("<p>hi</p>", "/x/e/p/b/", "{}").starts_with("<base"));
    }

    #[actix_web::test]
    async fn run_call_is_a_fresh_process_with_stdin_and_honest_exit() {
        let root = std::env::temp_dir();
        let r = run_call(Path::new("cat"), &[], Some("hello"), &root).await.unwrap();
        assert_eq!((r.code, r.stdout.as_str()), (Some(0), "hello"));
        // Non-zero exit is a result the frame reads, not a mechanism error.
        let r = run_call(Path::new("cat"), &["/nonexistent-file".into()], None, &root)
            .await
            .unwrap();
        assert_eq!(r.code, Some(1));
        assert!(r.stderr.contains("nonexistent"), "stderr comes back verbatim");
        // A binary that does not exist IS a mechanism error.
        assert!(run_call(Path::new("no-such-binary-xyz"), &[], None, &root).await.is_err());
    }
}

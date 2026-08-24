//! The shared edit module (V5.sv): the file-splicing primitives both the
//! CLI's authoring verbs and the daemon's edit-from-page endpoint use — the
//! move that ends the codebase's one dependency cycle (daemon called into
//! cli for these). Cross-cutting logic that owns no model: a plain file in
//! logic/, per the layer-first rationale (thread 126). The sidecar lock is
//! what SQLite transactions used to be — two writers splicing one file
//! unserialized would corrupt it — and temp-and-rename keeps the daemon's
//! next read untorn.

use anyhow::{bail, Context as _, Result};
use std::fs::File;
use std::os::fd::AsRawFd as _;
use std::path::Path;

use crate::format;

/// Take the page's sidecar lock: the serialization point for every splice,
/// CLI and daemon alike. Held until the returned File drops.
pub fn lock_page(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("sv.lock");
    let lock = File::create(&lock_path)
        .with_context(|| format!("creating {}", lock_path.display()))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        bail!("could not lock {}", lock_path.display());
    }
    Ok(lock)
}

/// Read-modify-write a page file under its sidecar lock, atomically. The lock
/// is what SQLite transactions used to be: subagents share a page id, and
/// two writers splicing the same file unserialized would corrupt it.
pub fn edit_page(path: &Path, f: impl FnOnce(String) -> Result<String>) -> Result<()> {
    let _lock = lock_page(path)?;
    let current = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let next = f(current)?;
    // Temp-and-rename in the same directory: the daemon's next read sees the
    // old file or the new one, never a torn one.
    let tmp = path.with_extension("sv.tmp");
    std::fs::write(&tmp, &next)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// The next `bN` id: one past the highest the file already uses, so ids never
/// recycle within a file even after `rm`.
pub fn next_block_id(page: &format::Page) -> String {
    let max = page
        .blocks
        .iter()
        .filter_map(|b| b.id())
        .filter_map(|id| id.strip_prefix('b'))
        .filter_map(|n| n.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("b{}", max + 1)
}

/// Append before a trailing `</sv-page>` when the file has one; a fresh file
/// gets the page wrapper so `page set` has a line to hang properties on.
pub fn append_block(current: String, block: &str) -> String {
    if current.is_empty() {
        return format!("<sv-page>\n\n{block}\n\n</sv-page>\n");
    }
    let mut lines: Vec<&str> = current.lines().collect();
    let closer = lines.iter().rposition(|l| l.trim_end() == "</sv-page>");
    match closer {
        Some(i) => {
            let mut new_lines: Vec<String> = lines[..i].iter().map(|s| s.to_string()).collect();
            while new_lines.last().map_or(false, |l| l.trim().is_empty()) {
                new_lines.pop();
            }
            new_lines.push(String::new());
            new_lines.extend(block.lines().map(str::to_string));
            new_lines.push(String::new());
            new_lines.extend(lines.drain(i..).map(str::to_string));
            new_lines.join("\n") + "\n"
        }
        None => {
            let mut out = current;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            format!("{out}\n{block}\n")
        }
    }
}

/// Replace (or with None, delete) a half-open line range. Collapses the
/// doubled blank line a deletion leaves behind.
pub fn splice(current: &str, (start, end): (usize, usize), replacement: Option<&str>) -> String {
    let lines: Vec<&str> = current.lines().collect();
    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    if let Some(r) = replacement {
        out.extend(r.lines().map(str::to_string));
    } else if out.last().map_or(false, |l| l.trim().is_empty())
        && lines.get(end).map_or(true, |l| l.trim().is_empty())
    {
        out.pop();
    }
    out.extend(lines[end.min(lines.len())..].iter().map(|s| s.to_string()));
    out.join("\n") + "\n"
}


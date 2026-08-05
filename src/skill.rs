//! The embedded skill: compiled from the same source tree as the binary, so it
//! can never describe flags the binary doesn't have. `skill install` writes it
//! out; re-running after an upgrade is the whole update procedure, and
//! `sideview status` says when the installed copy differs.
//!
//! Writing to `~/.claude/skills/` is a deliberate exception to "nothing in the
//! home directory": it is run once, by you, unsandboxed, and that path is
//! where Claude Code looks.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const SKILL_MD: &str = include_str!("../skills/sideview/SKILL.md");

/// What `sideview styles` prints. Deliberately small: the point of shipping a
/// framework the model already knows is that almost nothing needs documenting.
pub const STYLES: &str = "\
The page loads Bootstrap 5.3 (CSS only, no bootstrap.js) plus a prose layer.
Use the Bootstrap vocabulary you already know: grid (row/col-*), utilities
(d-flex, gap-*, mt-*, text-muted), components (alert, card, badge, table,
progress). Common Bootstrap 4 spellings (badge-danger, ml-*, text-left,
font-weight-bold) are shimmed, but prefer v5. JS-dependent components
(dropdown, modal, collapse) will not behave — use native <details> instead.

Bare semantic HTML is styled too: tables, blockquote, figure/figcaption, kbd,
pre/code look right with no classes, so markdown and markup share one look.
Light and dark follow the system theme via data-bs-theme — never hardcode
colors; use Bootstrap contextual classes or --bs-* variables.

sv- layer (domain elements with no precedent; still being derived from real plans):
  sv-metric      a headline figure, e.g. <span class=\"sv-metric\">3.2s</span>
  sv-delta       a change beside it, e.g. <span class=\"sv-delta\">→ 0.4s</span>
  sv-block       (structural — every block's wrapper; not for authoring)

Unknown classes render as no-ops; the daemon logs them.";

/// Every target harness speaks SKILL.md natively — there is no porting
/// problem, only an installation matrix (V1.md's harness section). A harness
/// counts as present when its binary is on PATH or its config dir exists
/// (auth'd tools have the dir before we ever look).
struct Harness {
    name: &'static str,
    bin: &'static str,
    /// User-level skills dir, relative to $HOME.
    skills_rel: &'static str,
    /// Presence signal besides the binary, relative to $HOME.
    config_rel: &'static str,
}

const HARNESSES: &[Harness] = &[
    Harness { name: "claude", bin: "claude", skills_rel: ".claude/skills", config_rel: ".claude" },
    Harness { name: "codex", bin: "codex", skills_rel: ".codex/skills", config_rel: ".codex" },
    Harness {
        name: "opencode",
        bin: "opencode",
        skills_rel: ".config/opencode/skills",
        config_rel: ".config/opencode",
    },
    Harness { name: "pi", bin: "pi", skills_rel: ".pi/agent/skills", config_rel: ".pi" },
];

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").context("$HOME is not set")?,
    ))
}

fn on_path(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|d| d.join(bin).is_file())
}

impl Harness {
    fn present(&self, home: &Path) -> bool {
        on_path(self.bin) || home.join(self.config_rel).is_dir()
    }

    fn skill_path(&self, home: &Path) -> PathBuf {
        home.join(self.skills_rel).join("sideview").join("SKILL.md")
    }
}

fn targets(agent: Option<&str>) -> Result<Vec<&'static Harness>> {
    let home = home()?;
    match agent {
        Some(name) => {
            let h = HARNESSES.iter().find(|h| h.name == name).with_context(|| {
                format!(
                    "unknown agent {name:?} — one of: {}",
                    HARNESSES.iter().map(|h| h.name).collect::<Vec<_>>().join(", ")
                )
            })?;
            Ok(vec![h])
        }
        // Every harness that's actually here; claude always (it is where this
        // project lives, and an empty machine should still get one install).
        None => {
            let mut out: Vec<&Harness> =
                HARNESSES.iter().filter(|h| h.name == "claude" || h.present(&home)).collect();
            out.dedup_by_key(|h| h.name);
            Ok(out)
        }
    }
}

pub fn install(project: bool, agent: Option<&str>) -> Result<()> {
    if project {
        // Project-level: one path, and it double-serves — opencode reads a
        // project's .claude/skills natively.
        let path =
            std::env::current_dir()?.join(".claude").join("skills").join("sideview").join("SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, SKILL_MD)?;
        eprintln!("installed {}", path.display());
        return Ok(());
    }
    let home = home()?;
    for h in targets(agent)? {
        let path = h.skill_path(&home);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, SKILL_MD)?;
        eprintln!("{:<9} {}", h.name, path.display());
    }
    Ok(())
}

pub fn uninstall(project: bool, agent: Option<&str>) -> Result<()> {
    if project {
        let path =
            std::env::current_dir()?.join(".claude").join("skills").join("sideview").join("SKILL.md");
        if !path.exists() {
            bail!("nothing installed at {}", path.display());
        }
        std::fs::remove_file(&path)?;
        let _ = std::fs::remove_dir(path.parent().unwrap());
        eprintln!("removed {}", path.display());
        return Ok(());
    }
    let home = home()?;
    let mut removed = 0;
    for h in targets(agent)? {
        let path = h.skill_path(&home);
        if path.exists() {
            std::fs::remove_file(&path)?;
            let _ = std::fs::remove_dir(path.parent().unwrap()); // only if empty
            eprintln!("removed {}", path.display());
            removed += 1;
        }
    }
    if removed == 0 {
        bail!("nothing installed for the selected harnesses");
    }
    Ok(())
}

/// For `sideview status`: one line per present harness. Drift is detected
/// rather than structurally impossible, which is the weaker guarantee
/// honestly labelled.
pub fn status_line() -> String {
    let Ok(home) = home() else {
        return "cannot resolve $HOME".into();
    };
    let mut parts = Vec::new();
    for h in HARNESSES {
        if h.name != "claude" && !h.present(&home) {
            continue;
        }
        let state = match std::fs::read_to_string(h.skill_path(&home)) {
            Err(_) => "not installed",
            Ok(s) if s == SKILL_MD => "current",
            Ok(_) => "STALE — re-run `sideview skill install`",
        };
        parts.push(format!("{}: {}", h.name, state));
    }
    parts.join(", ")
}

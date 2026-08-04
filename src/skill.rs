//! The embedded skill: compiled from the same source tree as the binary, so it
//! can never describe flags the binary doesn't have. `skill install` writes it
//! out; re-running after an upgrade is the whole update procedure, and
//! `sideview status` says when the installed copy differs.
//!
//! Writing to `~/.claude/skills/` is a deliberate exception to "nothing in the
//! home directory": it is run once, by you, unsandboxed, and that path is
//! where Claude Code looks.

use std::path::PathBuf;

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

fn skills_dir(project: bool) -> Result<PathBuf> {
    if project {
        Ok(std::env::current_dir()?.join(".claude").join("skills"))
    } else {
        let home = std::env::var_os("HOME").context("$HOME is not set")?;
        Ok(PathBuf::from(home).join(".claude").join("skills"))
    }
}

pub fn install(project: bool) -> Result<()> {
    let dir = skills_dir(project)?.join("sideview");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, SKILL_MD)?;
    eprintln!("installed {}", path.display());
    Ok(())
}

pub fn uninstall(project: bool) -> Result<()> {
    let dir = skills_dir(project)?.join("sideview");
    let path = dir.join("SKILL.md");
    if !path.exists() {
        bail!("nothing installed at {}", path.display());
    }
    std::fs::remove_file(&path)?;
    let _ = std::fs::remove_dir(&dir); // only if empty
    eprintln!("removed {}", path.display());
    Ok(())
}

/// For `sideview status`: drift is detected rather than structurally
/// impossible, which is the weaker guarantee honestly labelled.
pub fn status_line() -> String {
    let Ok(dir) = skills_dir(false) else {
        return "cannot resolve ~/.claude/skills".into();
    };
    let path = dir.join("sideview").join("SKILL.md");
    match std::fs::read_to_string(&path) {
        Err(_) => "not installed (run `sideview skill install`)".into(),
        Ok(s) if s == SKILL_MD => format!("installed, current ({})", path.display()),
        Ok(_) => "installed but differs from this binary — re-run `sideview skill install`".into(),
    }
}

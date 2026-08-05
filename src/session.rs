//! Which session am I? Load-bearing now that one daemon hosts many sessions:
//! get this wrong and two concurrent agents merge into one page.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub id: String,
    /// Which rung of the chain matched — recorded on the session row.
    pub detected_from: &'static str,
}

/// First match wins. The controlling tty is deliberately not on this list:
/// agent Bash calls have none, and each invocation is a fresh shell.
pub fn resolve(explicit: Option<&str>, cwd: &Path) -> Resolved {
    if let Some(id) = explicit {
        return Resolved { id: id.to_string(), detected_from: "flag" };
    }
    if let Ok(id) = std::env::var("SIDEVIEW_SESSION") {
        if !id.is_empty() {
            return Resolved { id, detected_from: "env" };
        }
    }
    // Subagents share this deliberately: $CLAUDE_CODE_CHILD_SESSION=1 arrives
    // with the *same* id, so a subagent's blocks land in its parent's page.
    if let Ok(id) = std::env::var("CLAUDE_CODE_SESSION_ID") {
        if !id.is_empty() {
            return Resolved { id, detected_from: "claude-code" };
        }
    }
    if let Ok(pane) = std::env::var("TMUX_PANE") {
        if !pane.is_empty() {
            return Resolved { id: format!("tmux{pane}"), detected_from: "tmux" };
        }
    }
    Resolved {
        id: format!("cwd:{}", cwd.display()),
        detected_from: "cwd",
    }
}

/// Are we inside an agent at all? Cheap, and only used for deciding whether
/// to attempt a browser launch versus just printing the URL.
pub fn inside_agent() -> bool {
    std::env::var_os("CLAUDECODE").is_some() || std::env::var_os("AI_AGENT").is_some()
}

/// Session ids appear as URL path segments and as page file names, and the
/// cwd and tmux rungs produce ids containing `/` and `%`. Encode everything
/// outside RFC 3986's unreserved set — over-encoding is harmless, a raw slash
/// is not, and the same encoding serving both uses means the file for a
/// session is recognizable from its URL.
pub fn encode(id: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SEGMENT: &AsciiSet =
        &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
    utf8_percent_encode(id, SEGMENT).to_string()
}

/// The throwaway page file for a session, relative to the project root: a
/// pure function of the session id, findable with no registry at all.
pub fn page_rel_path(id: &str) -> String {
    format!(
        "{}/{}/{}.sv",
        crate::store::DIR_NAME,
        crate::store::PAGES_DIR,
        encode(id)
    )
}

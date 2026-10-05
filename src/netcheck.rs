//! Can a socket bound here be reached from outside?
//!
//! Never "am I in a sandbox" — the property is directly observable: a
//! namespace with no non-loopback interface and no routes makes a daemon
//! unreachable by definition rather than by inference. The extra signals are
//! belt and braces; the asymmetry (a false positive costs one printed line, a
//! false negative costs an afternoon of inexplicable rendering) says prefer
//! refusing.
//!
//! A tell this module used to have, removed 2026-10-05 after it refused an
//! entire VM (found by the author's agent on exe.dev, sideview 0.6.2): an
//! inode threshold for "dynamically-created namespace" at 0xF0000000. On
//! kernels with fixed init-namespace inums (≈6.10+; this dev machine's init
//! net is 0xEFFFFFF9) the threshold holds, but on older kernels the init net
//! namespace takes the FIRST dynamic inum — exactly the boundary value — and
//! freed inums are reused, so no threshold is sound there. Its honest
//! replacement is the direct comparison below, used only when the kernel
//! lets an unprivileged process look; the other tells carry every sandbox
//! this project has actually met (bind probe: codex; interfaces/routes/
//! bwrap/uid_map: bwrap-style namespaces).
//!
//! When detection is still wrong somewhere new, `SIDEVIEW_ASSUME_REACHABLE=1`
//! overrides the verdict — the escape hatch exists so nobody patches vendored
//! source again.

#[derive(Debug, Clone)]
pub struct Verdict {
    /// The inode of `/proc/self/ns/net`, recorded on the daemon row so any
    /// later invocation can tell *exactly* whether the row describes a daemon
    /// in its own namespace.
    pub netns: Option<u64>,
    pub reachable: bool,
    /// Why not, when not — for the printed message.
    pub reasons: Vec<String>,
}

#[cfg(target_os = "linux")]
pub fn verdict() -> Verdict {
    let netns = ns_inode("/proc/self/ns/net");
    if std::env::var_os("SIDEVIEW_ASSUME_REACHABLE").is_some_and(|v| v == "1") {
        return Verdict { netns, reachable: true, reasons: Vec::new() };
    }
    let mut reasons = Vec::new();

    // The most decisive probe, and namespace-free sandboxes are why it must
    // come first: codex's landlock+seccomp denies socket() outright while
    // every namespace tell below reads clean — without this, the verdict
    // says reachable, the spawned daemon dies at bind, and the error lands
    // only in daemon.log. Measured live under `codex sandbox`, 2026-08-05.
    if let Err(e) = std::net::TcpListener::bind(("127.0.0.1", 0)) {
        reasons.push(format!("cannot create a listening socket ({e})"));
    }

    match non_loopback_interfaces() {
        Some(0) => reasons.push("no non-loopback network interface".to_string()),
        _ => {}
    }
    if let Some(false) = has_routes() {
        reasons.push("no routes".to_string());
    }
    // Compare to init's namespace when the kernel lets us look. Unprivileged
    // readlink of /proc/1/ns/* is usually denied on hosts, and inside a
    // pid-namespaced sandbox "/proc/1" is the sandbox's own init (equality
    // there says nothing — the tells above carry those cases); when it IS
    // readable and differs, that's a created namespace by definition.
    if let (Some(mine), Some(init)) = (netns, ns_inode("/proc/1/ns/net")) {
        if mine != init {
            reasons.push(format!(
                "network namespace differs from init's ({mine} vs {init})"
            ));
        }
    }
    if std::fs::read_to_string("/proc/1/comm")
        .map(|c| c.trim() == "bwrap")
        .unwrap_or(false)
    {
        reasons.push("pid 1 is bwrap".to_string());
    }
    if single_uid_map() {
        reasons.push("uid_map maps a single uid".to_string());
    }

    Verdict { netns, reachable: reasons.is_empty(), reasons }
}

/// Elsewhere, assume reachable and let the daemon's recorded flag be the
/// authority — macOS's Seatbelt creates no network namespace.
#[cfg(not(target_os = "linux"))]
pub fn verdict() -> Verdict {
    Verdict { netns: None, reachable: true, reasons: Vec::new() }
}

#[cfg(target_os = "linux")]
fn ns_inode(path: &str) -> Option<u64> {
    let target = std::fs::read_link(path).ok()?;
    parse_ns_inode(target.to_str()?)
}

/// The link target reads "net:[4026531833]".
#[cfg(target_os = "linux")]
fn parse_ns_inode(s: &str) -> Option<u64> {
    s.strip_prefix("net:[")?.strip_suffix(']')?.parse().ok()
}

#[cfg(target_os = "linux")]
fn non_loopback_interfaces() -> Option<usize> {
    let ifs = if_addrs::get_if_addrs().ok()?;
    Some(ifs.iter().filter(|i| !i.is_loopback()).count())
}

#[cfg(target_os = "linux")]
fn has_routes() -> Option<bool> {
    // /proc/net/route is a header line plus one line per v4 route.
    let content = std::fs::read_to_string("/proc/net/route").ok()?;
    Some(content.lines().skip(1).any(|l| !l.trim().is_empty()))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn ns_link_targets_parse_and_garbage_does_not() {
        assert_eq!(parse_ns_inode("net:[4026531833]"), Some(4026531833));
        assert_eq!(parse_ns_inode("net:[4026531840]"), Some(4026531840));
        assert_eq!(parse_ns_inode("mnt:[4026531840]"), None, "only net links");
        assert_eq!(parse_ns_inode("net:[]"), None);
        assert_eq!(parse_ns_inode("garbage"), None);
    }
}

#[cfg(target_os = "linux")]
fn single_uid_map() -> bool {
    let Ok(map) = std::fs::read_to_string("/proc/self/uid_map") else {
        return false;
    };
    let lines: Vec<&str> = map.lines().collect();
    if lines.len() != 1 {
        return false;
    }
    lines[0]
        .split_whitespace()
        .nth(2)
        .map_or(false, |count| count == "1")
}

/// Addresses worth binding beyond loopback under `--bind auto`: anything in
/// Tailscale's CGNAT range (100.64.0.0/10) or its v6 ULA (fd7a:115c:a1e0::/48).
/// Detected by address range, never by interface name — `tailscale0` is only
/// the conventional spelling and is wrong under userspace mode and on macOS.
pub fn tailnet_addrs() -> Vec<std::net::IpAddr> {
    let Ok(ifs) = if_addrs::get_if_addrs() else { return Vec::new() };
    ifs.into_iter()
        .map(|i| i.ip())
        .filter(|ip| match ip {
            std::net::IpAddr::V4(v4) => {
                let o = v4.octets();
                o[0] == 100 && (64..128).contains(&o[1])
            }
            std::net::IpAddr::V6(v6) => {
                let s = v6.segments();
                s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
            }
        })
        .collect()
}

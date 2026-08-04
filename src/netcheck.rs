//! Can a socket bound here be reached from outside?
//!
//! Never "am I in a sandbox" — the property is directly observable: a
//! namespace with no non-loopback interface and no routes makes a daemon
//! unreachable by definition rather than by inference. The extra signals are
//! belt and braces; the asymmetry (a false positive costs one printed line, a
//! false negative costs an afternoon of inexplicable rendering) says prefer
//! refusing.

/// The kernel's boundary between the initial and dynamically-created
/// namespaces. Real, but an internal detail — never leaned on alone.
const DYNAMIC_NS_BOUNDARY: u64 = 4_026_531_840;

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
    let netns = netns_inode();
    let mut reasons = Vec::new();

    match non_loopback_interfaces() {
        Some(0) => reasons.push("no non-loopback network interface".to_string()),
        _ => {}
    }
    if let Some(false) = has_routes() {
        reasons.push("no routes".to_string());
    }
    if let Some(inode) = netns {
        if inode >= DYNAMIC_NS_BOUNDARY {
            reasons.push(format!("dynamically-created network namespace ({inode})"));
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
fn netns_inode() -> Option<u64> {
    // The link target reads "net:[4026531833]".
    let target = std::fs::read_link("/proc/self/ns/net").ok()?;
    let s = target.to_str()?;
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

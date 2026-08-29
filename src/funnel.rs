//! Driving `tailscale funnel` for `sideview share` (V6.sv step 4). The
//! tailscale CLI is its only scriptable interface; every invocation here is
//! a fixed argv, no shell. Round 4's law governs the failure modes:
//! **detect and instruct** — at each one-time consent gate (funnel not
//! enabled on the tailnet, operator mode not set) sideview prints the exact
//! command or admin URL, verbatim, and stops. It never sudos and never
//! opens the admin console itself.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};

/// What `ensure` came back with.
pub enum Funnel {
    /// Serving: the public base URL, no trailing slash.
    Up(String),
    /// A gate the user must clear first — the instruction, ready to print.
    Blocked(String),
}

/// Ensure the funnel proxies `port`, without ever blocking on a consent
/// gate: `tailscale funnel --bg` polls forever while funnel is unenabled
/// (observed live, 2026-08-27), so the child gets a few seconds and is
/// then killed and judged by what it printed.
pub fn ensure(port: u16) -> Result<Funnel> {
    let mut child = match Command::new("tailscale")
        .args(["funnel", "--bg", &port.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Funnel::Blocked(
                "tailscale is not on PATH — sharing rides Tailscale Funnel; install tailscale and log in first"
                    .into(),
            ))
        }
        Err(e) => return Err(e).context("spawning tailscale"),
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    let exited = loop {
        match child.try_wait()? {
            Some(_) => break true,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut text);
    }
    Ok(judge(&text, exited))
}

/// The pure half, testable against captured transcripts.
fn judge(text: &str, exited: bool) -> Funnel {
    if let Some(url) = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("https://") && l.contains(".ts.net"))
    {
        // "Available on the internet: https://host.tailnet.ts.net/"
        return Funnel::Up(url.trim_end_matches('/').to_string());
    }
    if text.contains("not enabled") {
        let visit = text
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("https://"))
            .unwrap_or("the Tailscale admin console");
        return Funnel::Blocked(format!(
            "Funnel is not enabled on your tailnet (a one-time admin consent).\nEnable it at:\n  {visit}\nthen run this command again."
        ));
    }
    if text.contains("Access denied") || text.contains("operator") {
        return Funnel::Blocked(
            "tailscaled takes serve configs only from root or its operator (one-time):\n  sudo tailscale set --operator=$USER\nthen run this command again."
                .into(),
        );
    }
    if !exited {
        return Funnel::Blocked(
            "tailscale funnel did not come up within 5s — is tailscaled running and logged in? (`tailscale status`)"
                .into(),
        );
    }
    Funnel::Blocked(format!(
        "tailscale funnel answered unexpectedly:\n{}",
        text.trim()
    ))
}

/// `sideview share --off`: stop exposing anything. Best-effort; the report
/// is tailscale's own.
pub fn off() -> Result<String> {
    let out = Command::new("tailscale")
        .args(["funnel", "--https=443", "off"])
        .output()
        .context("running tailscale funnel off")?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(if text.trim().is_empty() { "funnel off".into() } else { text.trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_reads_the_three_transcripts_seen_live() {
        // Serving (also what an already-configured funnel prints).
        let up = "Available on the internet:\n\nhttps://lenovo.tail6804fa.ts.net/\n|-- proxy http://127.0.0.1:8899\n\nFunnel started and running in the background.\n";
        match judge(up, true) {
            Funnel::Up(u) => assert_eq!(u, "https://lenovo.tail6804fa.ts.net"),
            _ => panic!("should be up"),
        }
        // The tailnet-enable gate: the child polls forever, we killed it.
        let gate = "Funnel is not enabled on your tailnet.\nTo enable, visit:\n\n\thttps://login.tailscale.com/f/funnel?node=nqJEVvV1DQ11CNTRL\n";
        match judge(gate, false) {
            Funnel::Blocked(msg) => {
                assert!(msg.contains("login.tailscale.com/f/funnel"), "the admin URL verbatim");
                assert!(!msg.contains("sudo"), "wrong cure not offered");
            }
            _ => panic!("should be blocked"),
        }
        // The operator gate (seen live: enable succeeded, then denied).
        let op = "Success.\nsending serve config: Access denied: serve config denied\n\nUse 'sudo tailscale funnel --bg 8899'.\nTo not require root, use 'sudo tailscale set --operator=$USER' once.\n";
        match judge(op, true) {
            Funnel::Blocked(msg) => assert!(msg.contains("tailscale set --operator")),
            _ => panic!("should be blocked"),
        }
    }
}

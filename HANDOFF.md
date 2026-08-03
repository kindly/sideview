# Handoff

Written 2026-08-02, at the end of the design conversation that produced this repo. Revised
2026-08-03 after a review pass over all five documents, immediately before starting to code.
Everything here is either current state or something that exists nowhere else in the docs.

## State

Nothing is built. Six documents, an MIT licence, no remote.

The 2026-08-03 review changed V0.md in ways worth knowing about if you read it before then:

- **Sandbox detection is in, and `$CLAUDECODE` is not the test** — it is set both inside and outside
  the sandbox. The rule is now *auto-spawn unless the namespace is provably unreachable* (no
  non-loopback interface, no routes), with the daemon recording `netns` and `reachable` on its row.
  This closes a failure the design walked into: a daemon spawned inside a sandbox answers ping/pong
  perfectly and looks healthy to everything except the browser.
- **The CLI cannot escalate, but the agent can** — via the harness permission gate, re-running with
  the sandbox disabled, which is verified to produce a surviving host-bound daemon. The skill offers
  it; the printed instruction is the fallback. (A first pass of this review said escalation was
  impossible, conflating the CLI process with the agent driving it.)
- **Remote binding is tailnet-by-default** (`--bind auto`), because detection is free — interface
  enumeration measures 29µs in-process, once at daemon start, never on a CLI call. Detect by the
  `100.64.0.0/10` CGNAT range rather than the `tailscale0` name, print the raw IP rather than a
  hostname (this host's `hostname` is `cachyos-x8664`, which is *not* necessarily its MagicDNS name),
  never bind the wildcard, and fall back to loopback with a printed note when `bind()` gives
  `EADDRNOTAVAIL`. `--bind loopback` opts out; no token, with the revisit trigger being a tailnet
  node you don't control.
- **Worktrees resolve to the main checkout's store** via `--git-common-dir`, or the one-time daemon
  question becomes per-worktree.
- **`show` and the `image` block are cut entirely** (decided 2026-08-03, after the rest of this
  review). A front door with one type behind it isn't a front door, and `<img src="…">` in a markup
  block reaches the same result with nothing new to learn. The file endpoint and its root confinement
  stay, because `<img>` needs them. Three block types now, not four.
- **`markup` renders with no shadow root**, against DESIGN.md's rung-2 note.
- **A v0 schema exists** in V0.md, along with per-session `short_id`s and a defined fallback
  rendering for unknown block types.
- **`sideview daemon --restart` is gone**, incoherent with the daemon living in your foreground.
- **The Tailscale/token section is gone** — v0 binds loopback; `ssh -L` and `tailscale serve` need
  no code.

The design is settled enough to start coding from [V0.md](V0.md). It went through three reframes
getting there — from "richer plans than markdown", to "embed live pieces of a project", to "a
visual side channel for CLI agents whose content bypasses the model's context". The third is the
one the docs are written around, and it is the one that explains why the design looks the way it
does. Plans are now the flagship *use case*, not the definition.

## Do these before writing code

**Check whether `tailscale serve` buffers SSE.** Ten minutes, and it gates the entire remote story:
the product is one long-lived stream, and if Tailscale's reverse proxy buffers it despite
`X-Accel-Buffering: no`, then direct tailnet binding and `ssh -L` are the only remote paths and the
identity story goes with it. Stand up any trickling SSE endpoint behind `tailscale serve --bg` and watch
whether events arrive one at a time.

**Terminal graphics are no longer on this list.** An earlier version said to set up kitty's graphics
protocol first, on the grounds that if `kitten icat` already solved "show me a screenshot" then the
image block was solving a solved problem. With `show` and the `image` block cut, the item has lost its
reason — and it would have failed anyway: measured in an agent Bash call, `TERM=xterm-256color`, no
`KITTY_WINDOW_ID`, and stdout is a pipe with no tty, so the escape sequence never reaches the terminal
emulator. Terminal graphics need a live tty, which is the coupling sideview exists to avoid; it works
when *you* type the command, not when an agent runs one.

**The service-block spike, on the other hand, can wait** — the 2026-08-03 review argued for deferring
it and this section originally said the opposite. The spike itself is unchanged and still worth doing:
throwaway code, can a dev server be started, proxied, iframed into a page, and killed cleanly? It is
the long-term thesis and the only experiment that could reshape the roadmap.

But nothing in v0 depends on the answer, and the thing that will teach you most right now is the
latency feel and the class vocabulary against real plans — both of which need v0 running. So: **first
afternoon v0 is blocked on something else, do the spike.** The original argument for doing it first
was that discovering it in month three is expensive, which is true, and the counter is that week two
is early enough for a capability with no v0 dependents.

Optionally, an hour with [Wave Terminal](https://github.com/wavetermdev/waveterm) — its `wsh`
drives graphical blocks from the shell and is the closest existing thing to this idea. It was
rejected because the display is bound to its client app, but the ergonomics are worth feeling.

## Not documented anywhere else

**The host proxy DOES forward localhost — tested properly 2026-08-03, with a listener actually
running on the host.** An earlier version of this section concluded the opposite; it was wrong, and
the way it was wrong is instructive enough to record. What holds:

- The advertised `CLAUDE_CODE_HOST_HTTP_PROXY_PORT` (`39669`, `36113` — it varies) is **refused** from
  inside. The reachable endpoints are in-namespace `socat` forwarders on `127.0.0.1:3128` (HTTP) and
  `:1080` (SOCKS), which relay to a host-side proxy over a bind-mounted unix socket.
- They need **proxy auth**, and the credentials are in `$HTTP_PROXY` — regenerated per sandbox
  invocation, so nothing can be hardcoded.
- With `python3 -m http.server 8765 --bind 127.0.0.1` running on the host: **200 through both the HTTP
  and SOCKS proxies**, confirmed in the server's own access log. A raw TCP connect to the same address
  is refused, and so is anything that bypasses the proxy.

**Why the first attempt said "hang, therefore blocked":** nothing was listening on the ports probed
(`:9`, `:22`), and `$no_proxy` lists `127.0.0.1`, so curl silently ignored the `-x` proxy it was
supposedly testing and went direct. Both mistakes point the same way — if you re-test this, start a
listener first and clear `no_proxy` explicitly.

**What it changes, and mostly doesn't.** V0's core constraint is untouched: a daemon bound *inside*
the sandbox is invisible to the host in both directions (verified), so the browser still cannot reach
an agent-spawned daemon. What is now false is "the store is the only channel" — a sandboxed CLI can
HTTP a host daemon. V0 keeps the store as the mandatory path anyway (works everywhere, no credentials,
~100ms nobody perceives) and treats the proxy as an optional better liveness check.

**Escalation is real, via the agent rather than the CLI.** A `setsid nohup`'d listener started from a
sandbox-disabled Bash call binds a host port and **survives after the call returns** — verified. So
the skill can have the agent offer to start the daemon, which is one approval instead of a thing you
type. The CLI process itself still cannot escalate and never prompts.

**One stray process found while testing.** An orphaned `bwrap` from an earlier session is still
running `python3 -m http.server 41777` with a `sideview-bind-test-ok` index, rooted at
`/home/david/compuse` — a leftover from a previous bind experiment. Harmless, but it is the exact
failure mode the design's "teardown validates the lifecycle decision" note is about, arriving before
any code was written.

**The sandbox measurements, for reference**, taken the same day from one sandboxed Bash call and one
with the sandbox disabled. The full table is in V0.md; the short version is that the sandbox has only
`lo` and no routes, `/proc/1/comm` is `bwrap` at pid 2, `uid_map` is `1000 0 1`, and the net-ns inode
is `4026532958` against the host's `4026531833`. `$CLAUDE_CODE_SESSION_ID` is present and stable
(Claude Code 2.1.220), and `$CLAUDE_CODE_CHILD_SESSION=1` accompanies the *same* session id in
subagents.

**Naming research, so it isn't repeated.** `sideview` is free on crates.io. Also checked:
`showme` is taken by a terminal image viewer (adjacently confusing), `vitrine` by a static site
generator, `glance` by a computer-vision crate. `agentview` and `viewfinder` are free on crates.io,
but `agentview` is badly crowded — `agentview/agentview` is a session viewer for conversational
agents and `kenn-io/agentsview` does session analytics for coding agents, both of which are the
"transcript mirror" category sideview is explicitly *not*. The name was chosen to encode the
thesis: a view beside your terminal, fed by a side channel.

**The crate name is unclaimed and the repo has no remote.** Publishing is deliberately left to a
human decision. `gh repo create` when ready.

**LICENSE says "David Raznick" personally**, not Global Energy Monitor. Change it if that's wrong —
it was a judgement call based on this being a personal project directory.

**`~/.claude/skills/hunk-review` is currently a broken symlink** (into a `hunkdiff` npm package
that has moved). Noticed while researching how to ship sideview's own skill, which copies that
distribution model. Worth fixing independently.

## Open, and deliberately so

**Scroll behaviour when a block arrives.** Likely answer: scroll only when already at the bottom.
Left unspecified because it wants a real page in front of you.

**The `sv-` class list.** Six to ten classes for what Pico and Bootstrap naming don't cover —
metric/delta, option cards, decision matrix. Needs designing against real plans, not in the
abstract. The same now applies to *which* Bootstrap names to implement: the review moved that from
"the common subset" to a thing to derive from three or four plans an agent actually wrote.

**Which DESIGN.md sections are stale** — each now carries a marker in place, so this list is only a
map: the schema sketch (predates the cut), "Identifying the session" (tty-based chain), "Lifecycle"
(per machine, idle exit), rung 2's shadow root, and build order. Reconcile them properly when v0
ships rather than now.

## The conversation's own summary

If a future session wants to know *why* rather than *what*, the reasoning is in the docs rather
than in any transcript — every significant decision was written down with the alternative it beat.
That was deliberate. The most load-bearing pieces of reasoning, in rough order:

1. Content must not pass through the model's context. Everything else follows.
2. The sandbox gives each Bash invocation its own network namespace, so the agent cannot start a
   reachable daemon.
3. A design vocabulary only saves tokens if the model already knows it.
4. If a page has no live blocks, markdown was already the right answer.

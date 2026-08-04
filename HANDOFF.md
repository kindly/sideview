# Handoff

Written 2026-08-02, at the end of the design conversation that produced this repo. Revised
2026-08-03 after a review pass over all five documents, immediately before starting to code.
Everything here is either current state or something that exists nowhere else in the docs.

## State

**The skeleton exists (2026-08-03).** A compiling crate with the v0 shape end to end: spec
types with envelope-first decoding, the store with `user_version` migration and the daemon
row, session resolution, the netns reachability verdict, server-side rendering (comrak GFM),
the actix daemon with SSE + `Last-Event-ID` replay, the file endpoint with root confinement,
the flock auto-start, the plain-JS frontend, and the embedded skill with `skill install`.
Eleven unit tests pass, and the following were verified live on this machine:

- **Unsandboxed authoring auto-spawns**: `sideview prose` with nothing running wrote `b1`,
  spawned a detached daemon under the flock, and the page + SSE replay served the rendered
  block. Tailnet auto-bind picked up both the CGNAT v4 and the ULA v6 address.
- **Sandboxed authoring refuses correctly**: block written, id printed alone on stdout, the
  one-line instruction on stderr, exit 0, no spawn attempted.
- **Supersession works as specified**: a second daemon claimed the row and the first evicted
  itself within a heartbeat; SIGINT cleared the row and left no processes.
- **One wrinkle found**: during a *live* takeover the new daemon cannot reuse the remembered
  port — the old daemon still holds it at bind time, so the second falls back to an ephemeral
  one. "The recorded port is reused" therefore holds across restart-after-exit (the case that
  matters for SSE reconnection) but not across supersession. Acceptable; noting it so the
  claim in V0.md's port section is read with that asterisk.

Deliberate skeleton gaps, all marked `TODO(v0)` in source or visible in the code: unknown
class/`style=` logging (unblocked once `scraper` landed on 2026-08-04, still unbuilt), iframe
autosizing (fixed 24rem until ResizeObserver + postMessage), the spawn-lock release window
between CLI exit and the child's claim (healed by supersession), and scroll behaviour is the
provisional only-when-at-bottom guess in app.js. The `tailscale serve` SSE-buffering check below
is still not done, and the "Done when" regression trap (a stale row from a namespaced daemon,
claimed by bare `sideview`) has never been staged. (Session labels gained a writer on 2026-08-04
— `session set` — and the two code-review bugs, the swallowed SSE `Lagged` error and unencoded
session ids in printed URLs, were fixed with pinning tests before the first commit.)

**Later the same day, the design system switched to real Bootstrap 5** — vendored v5.3.8, CSS
only, with a prose layer for bare markdown elements and a v4-compat shim in `sideview.css`.
Pico is gone. V0.md's design-system section records the reversal, why the borrowed-subset
approach lost, and the Tailwind/daisyUI rejection.

**2026-08-04: blocks declare their headings, and the page grew an outline sidebar.** The SSE
event carries `headings` (see V0.md's frontend section for derivation rules per type). The
outline is optional from both sides — a viewer toggle in the header, and agent-side
`sideview session set --label … --outline auto|off`, which brought session properties in as one
chunk and finally gave `label` a writer. Properties then became a single JSON `props` field
(reasons in V0.md — chiefly that every `user_version` bump hard-stops older binaries, too high a
price per cosmetic flag). The interim migration steps that got here were **squashed back to a
single v1 the same day**, per V0.md's pre-release rule, and the one existing store was deleted by
hand — but not before the machinery had been exercised for real on live data: `user_version`
stepping, the pre-migration backup on a non-additive step, and a column-fold via `json_patch` all
ran and worked. Two knock-on facts worth knowing: `scraper` is now a dependency, which unblocks
the unknown-class/`style=` logging TODO in `render.rs` — the lenient HTML parser it was waiting
for is in the tree; and dogfooding this found two more bugs, both fixed: the remembered port lived
only on the daemon row so a *clean* restart forgot it (now durable in the `meta` table), and CLI
output piped into `head` panicked on EPIPE (SIGPIPE now restored to default). Same session,
earlier: `shutdown_timeout(1)`, because a page holding its SSE stream open otherwise made every
daemon shutdown take the full 30s grace period.

Otherwise: six documents, an MIT licence, no remote.

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

**Sessions cannot be deleted, and the design labs made that visible.** The theme/font/combo lab
sessions (2026-08-04) did their job and now sit in the switcher forever: `rm` tombstones blocks
but nothing removes a session, and hard-DELETEing rows by hand would regress `MAX(rev)` and
corrupt `Last-Event-ID` replay — the rev counter must survive any future deletion feature (a
`meta`-held floor, or tombstoned sessions). A `session rm`/archive belongs in the next batch of
session work; until then, labs cost a permanent chip each.

**The file endpoint serves `.sideview/` itself** — noticed 2026-08-04 when `/f/` got its first
real use (serving the dogfood experiment's HTML over the tailnet, which worked perfectly).
`.sideview/` is inside the project root, so `/f/.sideview/sideview.db` is fetchable by any
tailnet node. Within v0's accepted exposure model, but excluding the store's internals from an
endpoint that exists for project content is a five-line fix worth making.

**First controlled dogfood (2026-08-04):** three identical subagents summarized this project
visually — one on sideview (given nothing but the installed skill), one as a local HTML file,
one as a published Claude Artifact. Sideview: 3:04 total, **first content on screen at 71s**,
then a block every ~15–20s; 73k tokens. Artifact: 4:20, nothing visible until done; 80k tokens.
Local HTML: 6:48, nothing until done; 96k tokens (it hand-rolled an entire design system —
exactly the cost V0.md's premise predicts). n=1, agents varied in self-QA thoroughness, so the
totals are indicative; the *shape* (streaming vs single reveal, styled-for-free vs
invent-your-own-CSS) is structural. Skill-tuning observation: the sideview agent wrote 6 of 7
blocks as `markup` rather than `prose` — if prose-first is wanted, the skill has to say so.

**Rounds 2 and 3 (same day) turned the experiment into a tuning loop, and the loop has a cost
curve.** Each round's visual gaps were named, fixed (Plex + print duotone, deeper paper, bigger
display, SVG-first diagram guidance, themed mermaid), and re-tested. Result: sideview's looks
converged toward the leaders while its headline metric inverted — time-to-first-content went
71s → 106s → 261s as the skill demanded more craft, and in round 3 the artifact beat sideview
on *total* time (5:08 vs 5:53) for the first time. The quality guidance taught agents to
compose before emitting, which is exactly what streaming exists to avoid. Resolved after the
round-3 verdict (sideview's SVG diagram judged best of the three; overall still behind local
HTML's flourishes, near-parity with the artifact): the skill now matches effort to the page's
job — working plans stream prose-first with diagrams explicitly optional (mermaid sketch when a
picture genuinely helps), and hand-authored SVG is reserved for pages whose point is visual
presentation — plus an explicit stream-it instruction: emit early, sharpen with `update`.
Comparison pages: sessions `round2`, `round3` (tabs mode, rivals iframed via `/s/` and `/f/`).

**The round-1 author's verdict on looks inverted the speed ranking: local HTML best, artifact
middle, sideview worst.** Three causes traced, each actionable. (1) The HTML agent invoked the
frontend-design skill (transcript-verified; the others didn't) and had free rein, where
sideview's skill deliberately enforces the house style — speed and consistency bought at a
polish ceiling. (2) Sideview's weakest elements were its hand-drawn diagrams, which makes
**mermaid the deferral with the strongest evidence against it** — V0.md's Out list says "real
demand, but not this version"; the demand is now measured, not predicted. (3) The agent
confidently presented `sideview show readings.parquet` as the pivotal property because
**README.md still said so** — the 2026-08-03 cut reached V0.md and HANDOFF but never the front
door. Fixed same day. The doc-rot lesson generalises: a cut isn't done until the docs that
*sell* the feature are updated, not just the ones that specify it. app.js is
~330 lines of hand-rolled DOM state sync and growing; the itch for a no-build framework
(Alpine, petite-vue, Vue's ESM build) is legitimate. Deliberation so far: two different JS
domains are conflating. The *page chrome* (rail, strip, spy, dot) is our code and small —
vanilla holds until the feedback channel lands, whose forms and params are the first genuinely
framework-shaped work; that's the natural adoption point, and Alpine or Vue-ESM (both no-build,
vendorable as one file, deep LLM priors) are the candidates — petite-vue is unmaintained, htmx
overlaps what SSE already does here. The *plugin architecture* (blocks getting scoped access to
parts of the page) should not be answered with a framework at all: the web-native boundary is
custom elements plus a small explicit `window.sideview` API, which keeps plugins
framework-agnostic, gives them shadow-DOM isolation (DESIGN.md's rung-2 note returns here), and
lets an agent emit `<sv-something>` as ordinary markup. Constraint to hold either way: whatever
is adopted must vendor as a single static file into rust-embed — no toolchain, per V0.md's
frontend section.

**React-controlled blocks, a ladder not a decision** (2026-08-04, prompted by wanting a
Glide-grid table like querier's). React never controls the page — per-block roots or iframes
only. Rungs, cheap to expensive: (0) *already works*: a Vite `dist/` copied into the project and
iframed via the file endpoint (`/f/…`, build with `base: './'`) — this is also the service-block
spike arriving through the front door, since an iframe can equally point at a running app's own
port; (1) the deferred `table` block ships as a sideview-precompiled custom element wrapping
Glide, vendored like Bootstrap — node becomes a maintainer-time toolchain, the one-binary user
promise survives, and `{sql}` finally exercises reference-never-embed; (2) pane takeover is just
a session property in the props bag (no migration) — one block filling the viewport below the
header; (3) artifacts parity — agents writing TSX against a pinned import map of vendored ESM —
would use SWC embedded in the daemon (Rust, transpile at write time, browser runs native
modules), not a browser-side Babel. Take rung 0 as an early experiment; take rung 3 only if 0–2
prove insufficient.

**The `sv-` class list.** Six to ten classes for what Bootstrap doesn't cover — metric/delta,
option cards, decision matrix. Needs designing against real plans, not in the abstract. (The
companion question — *which* Bootstrap names to implement — dissolved on 2026-08-03 when the
design switched from a borrowed subset to vendoring real Bootstrap 5; see V0.md. What remains
derivable from real plans is the `sv-` layer and any v4-shim additions the unknown-class logs
reveal.)

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

# Handoff

Written 2026-08-02, at the end of the design conversation that produced this repo. Revised
2026-08-03 after a review pass over all five documents, immediately before starting to code.
Everything here is either current state or something that exists nowhere else in the docs.

## State

**v2 built on branch `v2`, overnight 2026-08-07 (agent, for the author's morning review) —
unreleased, unmerged.** Everything V2.sv commits to except watched diffs (re-sequenced out by
the author) is implemented and green (43 tests): migration v2 (sessions→bindings + the
threads/comments/outlines tables), `page set/rm/promote` with session aliases, `open <file>`,
`comment`/`resolve --undo`/`watch --claim` (typed JSON-lines; the whole loop verified live —
CLI comment → watch replay → live resolve event → exactly-once claims), the browser endpoints
plus a per-page `threads` SSE snapshot, the margin-mark/count-dot/popover/tail-list UI, the
iframe envelope (size out, theme in — 85vh retired), sv-note rendering, explicit outlines
verbatim in the rail, and startup rediscovery — the resurrection test ran live and passed
(db deleted, page back from canon, conversation gone). Binary installed and both project
daemons restarted on it; both stores migrated with `-pre-v2` backups beside them.

Honest gaps for the review: the comment UI has had no human visual pass (built to the CSS
system, never seen by eyes); paragraph anchors hash in JS only (`anchorHash`, FNV-1a 64/48,
vector pinned in app.js — the rust twin belongs with diff re-resolution, which didn't start);
`l:` anchors and per-line diff comments are unimplemented; sv-note renders in place with a
reference line rather than physically at its anchor; explicit outlines assume scrollspy
(tabs+spec degrades to all-visible); V2.sv's sign-off ritual (comment every heading from the
browser, agent picks each up via watch) awaits the author.

## Older state

**v1 shipped: 0.1.0 published to crates.io, 2026-08-07.** Every done-when bar in V1.md was
met: pages are files (verified by deleting the only db, twice), broken files heal on save,
deletion is file removal from page and CLI, code highlights in both themes, multi-file diffs
render unified and side-by-side with rail navigation, and the four-harness matrix ran live —
all on OpenAI models — with the author manually confirming all three foreign harnesses before
release. v2's core is already designed with reasons attached (V1.md's committed-to-v2
entries): watched diffs via gitoxide, comments in the db behind `sideview watch`, Sphinx-style
hover placement, explicit agent outlines, and the dividing principle that governs them all —
sv files are version-control-worthy canon; the db holds what should not be versioned.

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

**The v0 → v1 line, drawn by the author on 2026-08-04.** v0 closed with the file-endpoint
exclusion and `--detach` printing the tailnet URLs. Moved to v1: iframe autosizing (fixed 24rem
until ResizeObserver + postMessage), staging the "Done when" regression trap (stale namespaced
row, claimed by bare `sideview` — implemented, never staged), the third done-when bar (skill
offers the sandbox-disabled `--detach` with nothing running), and the `tailscale serve`
SSE-buffering check. **v1 itself is the real dogfood**: genuine work sessions writing genuine
plans through the skill — every experiment so far was showcase-shaped, and the product is plans.
Deprioritized rather than moved: unknown-class/`style=` logging — full Bootstrap made silent
no-ops mostly moot; the `TODO` stays in render.rs for when the vocabulary-data curiosity
returns. Still open by design: the spawn-lock release window (healed by supersession) and the
provisional scroll behaviour. (Historical: session labels gained a writer and the two
code-review bugs — swallowed SSE `Lagged`, unencoded session ids — were fixed with pinning
tests before the first commit.)

**v1 was scoped the same evening (2026-08-04), by the author, then re-founded within hours:
[V1.md](V1.md).** The goal is 0.1.0 on crates.io. Dogfooding the plan immediately exposed the
canonicality dichotomy (the plan existed as V1.md *and* as page blocks — which is correct?),
and the discussion landed somewhere bigger than the original scope: **pages are files.** Every
page's canonical source becomes a `.sv` block document — throwaway ones implicit in
`.sideview/pages/`, document ones committed in the repo — with the db demoted to daemon
bookkeeping, bindings and derived replay state ("delete the db and no content is lost"). The
rest of v1 lands on top: session deletion (= deleting the file; three earlier designs and the
whole rev-counter problem dissolved — see V1.md), harness independence proven live in
codex/opencode/pi, code highlighting (syntect, class-based, duotone), and diff blocks
(`git diff | sideview diff`, file paths as outline headings). Explicitly deferred: tables and
app subprocesses. Also set at scoping, author's rule: **dogfood first** — nothing is
implemented before its design has appeared on the live page.

**2026-08-05: the substrate landed — pages are files, live.** The format survived a full day
of adversarial probing (V1.md's stress-test section) before a line was written; the author's
sign-off included deleting the only store, so there was no migration — the v0 schema is simply
gone. What shipped: `format.rs` (the fence scanner, with implicit-close healing and the
column-0 rule, which the plan page itself needed on day one for its own format examples),
authoring as locked atomic file splicing, the daemon rebuilt around in-memory state derived
from files (stat-polling bindings, reparse-diff by id, full-state SSE connections — which
dissolved `Last-Event-ID`, tombstones and the rev counter in one move), and `spec.rs` deleted.
Verified live: CLI append/rm round-trip through the file; a raw `sed` on the page file patched
exactly one block over SSE; the db was deleted and rebuilt with the page content intact —
the delete-the-db test passed for real. The skill gained its your-page-is-a-file paragraph
(direct edits are equivalent to the CLI; never escape inside a block; tags count at column 0)
and was re-installed current.

**Code highlighting landed 2026-08-05**: syntect through comrak's adapter (`syntect-fancy`,
pure Rust), class-based with an `sv-` prefix, in the same render pass as the markdown. The
duotone treatment lives in sideview.css — keywords/storage in ink, operators deliberately
exempted (inking every `=` is noise), entities by weight not color, strings/comments in grays,
one rule set for both themes since every color is a token that swaps. Mermaid fences keep
their `code.language-mermaid` contract (the client reads `textContent`, so syntect's spans are
harmless) — test-pinned. Cost: +1.1 MB on the binary (16.2 → 17.3 MB), the embedded default
syntax set; fine against crates.io's 10 MB *package* cap since the syntax set ships inside
syntect, not our crate.

**Diff blocks landed 2026-08-05**: `git diff | sideview diff`, the fourth block type. diffy's
`PatchSet` parses (git extended format: renames, creates, binary entries — all titled
honestly); the aligned model is ours (removed/added runs paired index-wise, unpaired lines
against empty cells); `similar` marks word-level `<del>`/`<ins>` on paired lines, gated by
*character*-level ratio ≥ 0.4 — word tokenization counts whitespace as matches and flatters
unrelated lines, a bug the tests caught on day one. Both views render into one HTML string
(inactive hidden by `data-view`); `view=` on the block is the agent's default, the client
toggle is the viewer's override remembered per block, narrow screens collapse to unified.
File paths are outline h2s with anchors — the rail navigates a multi-file diff (verified live
on the session-deletion commit itself: 8 files, 8 rail sections). Garbage degrades to raw
mono with an honest note; a mid-diff parse failure renders the files that parsed plus a
visible "rest could not be parsed". Duotone tints: additions lean ink, removals lean the warm
tone, intraline is a deeper wash of the same. Deferred within diff (V1.md): syntax
highlighting inside lines, `src=` references; watched diffs are committed to v2 via gitoxide.

**The mobile diff saga (2026-08-05, evening) — six distinct causes, worth remembering.** The
author's phone (Chrome on iOS) showed the diff oversized with wrapping numbers, and the fix
took six real findings, pinned by a live probe block reporting computed styles from the
device: (1) number cells inherited `pre-wrap`/`break-word` and wrapped digits; (2) below 992px
the rail bows out but `body.sv-rail #sv-blocks` kept 6rem of desktop side padding — pure
phantom gutter; (3) embedded assets change on upgrade behind unchanged URLs, so phones showed
stale CSS — assets now send `Cache-Control: no-cache`; (4) **WebKit text autosizing inflates a
*container's* computed font-size and lets inheritance carry the boost into children, even with
`text-size-adjust: none` set and reported — but an element's own rem declaration computes
against the root and escapes.** Any deliberately small type must be declared on the element
holding the text, not inherited (this is why the diff font "never changed" through three
attempts). (5) Empty cells have no line box, so blank diff lines rendered squashed —
`td:empty::before { content: "\00a0" }`. (6) The prose layer's `.sv-block td` (equal
specificity, later in the file) silently beat every `.sv-diff-table td` padding on *all*
platforms — diff tables are now excluded from prose-table treatment via `:not()`. Landed
sizes: 0.8rem desktop, 0.78rem mobile with one-line-per-row horizontal scroll inside the
figure, 1px vertical cell padding so consecutive intraline washes don't fuse.

**The harness matrix began 2026-08-05 (evening).** codex 0.146.1, opencode 1.18.13 and pi
0.73.1 are installed via npm (auth pending — OAuth flows need the author at the machine);
`sideview skill install` reaches all four harnesses and `status` reports per-harness drift.
First hard finding, measured under `codex sandbox` before any auth existed: **codex's
landlock+seccomp denies `socket()` outright** — no network namespace, so every one of
netcheck's namespace tells reads clean, the old verdict said "reachable", and a spawned
daemon would have died at bind with the error visible only in daemon.log. The verdict now
leads with a decisive universal probe (bind a loopback listener; on error, refuse with the
reason) — sideview under codex-default now prints the honest one-line instruction, same as
under Claude. Still open for the authed runs: whether codex with `network_access=true`
lets auto-spawn *work* (no namespace means a permitted socket is genuinely reachable),
opencode and pi end-to-end (neither sandboxes by default, so auto-spawn should just work),
and their session-identity env vars. `codex sandbox`'s default is read-only fs, so the
store-write path also waits for the real `codex exec` run.

**The harness matrix ran live 2026-08-06 — all three legs pass, all on OpenAI models** (the
author's provider choice, which made it a cross-model-family test of the skill and format).
Per harness, in a shared scratch project, each run's env captured to a file for ground truth:

- **codex** (exec, `--full-auto`; its default exec sandbox is read-only and even blocks
  `env > file`): skill activated, two blocks + label written, and the netcheck socket-probe
  fix fired exactly as designed — the honest "no daemon running — run `sideview` in …" line
  under a sandbox that denies `socket()`. No session id in its shell env (`CODEX_THREAD_ID`
  is MCP-only in 0.146); falls to the cwd rung.
- **opencode** (run): skill activated, and **auto-spawn worked — the first agent-started
  daemon in the project's history** (no namespace, permitted sockets; pid claimed the row,
  page served 200, the browser tab opened on the author's desktop, which unsandboxed is the
  right UX). Exposes `OPENCODE_PID` → now a session rung (`opencode:<pid>`).
- **pi** (`-p`): skill activated, block written against the already-running daemon (third
  daemon path, silent success). Markers only (`PI_CODING_AGENT`), no id → cwd rung.

Findings that became code: the `OPENCODE_PID` rung; `--session ''` (a codex model invented
the spelling) minted an empty-id session — empty explicit ids now fall through. Findings
recorded, not coded: env-less harnesses sharing a project share the cwd session (pi's label
overwrote codex's mid-matrix — coarse identity working as designed); one contaminated first
run (launching codex from inside Claude Code leaks `CLAUDE_CODE_SESSION_ID` — matrix runs
must scrub the env). Claude Code's own leg is this entire project's history.

**Session deletion followed the same day.** Deleting a page is deleting its file: `sideview
session rm [id]` (no id = your own; never auto-spawns a daemon) and `DELETE
/api/sessions/{id}` — the page's first write, behind the ✕ on the session chip, two-step and
self-disarming, tidying power rather than authoring power. Both remove file + sidecar lock +
binding; the poll loop notices the binding vanish, and every tab converges because the client
now treats the sessions snapshot as authoritative (blocks of unlisted sessions are dropped —
also what heals a tab that slept through a deletion). Verified live in both directions; the
scratch sessions used for the test were themselves deleted through the feature. 28 tests.

**Later the same day (2026-08-04), the design system switched to real Bootstrap 5** — vendored v5.3.8, CSS
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

**The file endpoint no longer serves the store's internals** — noticed 2026-08-04 when `/f/`
got its first real use (`/f/.sideview/sideview.db` was fetchable by any tailnet node), fixed
same day: `sideview.db*` (backups included), `daemon.log` and `spawn.lock` return 403 by name,
while other files under `.sideview/` still serve — the dogfood comparison pages iframe their
rival entries from there. Pinned by test.

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
overlaps what SSE already does here. **Update (2026-08-08): the trigger fired** — the feedback
channel landed and the popover/tail code is exactly the predicted framework-shaped work; a hard
day of scroll debugging also showed the block-reconciliation bugs belong to a different fix
(idiomorph-style morphing, on V2.sv's candidates). Leading candidate when adoption is chosen:
Vue-ESM islands for the conversation UI only, blocks staying vanilla — the author's other tools
are Vue, and Vapor mode was assessed as no threat to the no-build path (opt-in, build-time, same
authoring model, and a vendored file can't rot). Composition API works fully in the ESM build —
it's only the `<script setup>` sugar that needs a compiler (write `setup()` + explicit return);
one real caveat: the runtime template compiler uses `new Function`, so a strict CSP without
unsafe-eval would block it — remember this if sharing ever grows CSP headers. **The author's
reframe (2026-08-08): the main prize is html blocks as Vue islands** — a vendored
vue at /assets/vendor/ is importable by any srcdoc block, giving artifact-grade interactive
blocks with no CDN, no in-browser JSX transpile, files-in-repo persistence, and the envelope
already sizing/theming them. This benefit arrives from vendoring alone, before any migration
of our own UI. Mechanics: opaque-origin iframes need Access-Control-Allow-Origin: * on
/assets for ESM imports (one safe line), or blocks use the global build via script src. Adoption itself remains the author's call;
vanilla currently holds. The *plugin architecture* (blocks getting scoped access to
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

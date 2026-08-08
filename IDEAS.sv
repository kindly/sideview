<sv-page label="ideas — the pool v3 draws from">

<sv-prose id="intro">
# The pool — everything discussed, still unbuilt

Every idea and piece of upcoming work from V0/V1/V2, DESIGN, SHARING, PRIOR-ART and HANDOFF, in one place, so v3 is curated by **adding to and removing from this page**. One line per idea; the reasoning lives in the named doc — reopening anything deferred twice stays a deliberate act (V2's rule). Comment on a bit to argue it in or out; what survives moves to V3.sv's In list.
</sv-prose>

<sv-prose id="feedback-finish">
## Finishing the feedback loop (v2's honest gaps)

- **Per-line diff comments** (`l:` anchors) — line fingerprint + two lines of context, three-state re-resolution (exact / fuzzy-with-marker / orphan); design complete and watched-ready, unimplemented. Repriced 2026-08-09: selection quotes already survive inside diffs, so this buys precise jump-back, not comprehension. *(V2.sv feedback; V1.md)*
- **Per-line comments inside any code block** — the `l:` machinery generalized to any pre. Deferred twice. *(V2.sv candidates)*
- **Rust twin of `anchorHash`** — paragraph hashing is JS-only (FNV-1a 64/48, vector pinned in app.js); the daemon side belongs with diff re-resolution. *(HANDOFF gaps)*
- **sv-note placed physically at its anchor** — today it renders in place with a reference line. *(HANDOFF gaps)*
- **Comment image attachments** — paste/drop into the compose card → daemon writes a project file, the comment stores the path; thumbnail in the card, watch names the path, the agent reads it with its file tool. Reference-never-embed extended to conversation; genuine vision, pull not push. *(V2.sv candidates, 2026-08-09)*
- **Mobile comment pass** — touch has no hover: marks faintly visible, tap-to-reveal; a small design pass of its own. *(V1.md)*
</sv-prose>

<sv-prose id="live-content">
## Live content

- **Watched diffs** — `src="git:…"` re-diffed on the poll tick; **gitoxide, never a git subprocess** (attributes/textconv = code execution); constrained revspec grammar. Proven possible, re-sequenced out of v2. *(V1.md diff section; V2.sv candidates)*
- **Sidecar log tailing** — a block names a file the daemon tails; any process (`pytest >> …`, CI, a build) can feed a plan with no agent in the loop. The sleeper feature. *(DESIGN.md on-disk)*
- **Patch-in-place block rendering** — idiomorph-style morphing instead of wholesale replacement; restores native scroll anchoring (the client-side reading anchor covers the symptom today). *(V2.sv candidates)*
- **Syntax highlighting inside diff lines** — syntect is already in the render pass. *(V1.md)*
</sv-prose>

<sv-prose id="blocks">
## New block types

- **Tables** — `{sql}` finally exercises reference-never-embed; DuckDB as the engine (parquet/CSV/SQL, 100k+ rows). Grid verdicts recorded 2026-08-08: VisActor VTable first (pure-JS canvas, UMD-vendorable, framework-neutral, async data model), AntV S2 runner-up, Glide disqualified (React-only) — and embedding a live sqlnow session may beat a native block for the explore case. *(V2.sv candidates; DESIGN.md; HANDOFF)*
- **sv-tree** — a nested markdown list *is* a tree: comrak parses it, CSS draws connector elbows, no layout engine; degrades to an indented list on old binaries. Sequence diagrams the plausible second resident; real DAG layout stays extension territory. *(V2.sv candidates)*
- **Chart, params form, choose-between-options, query-plan viewer** — the tier-2 wishlist, never scoped. *(DESIGN.md blocks)*
- **An image/figure block returning** — styled `<figure>` + alt handling; re-read V0's `show` front-door argument before reinventing the command. *(V0.md Out)*
- **Terminal/PTY block** — where websockets finally earn a place; never shared, at any tier. *(DESIGN.md; SHARING.md)*
</sv-prose>

<sv-prose id="apps-extensions">
## Apps, services, extensions

- **Service blocks** — supervise → endpoint: the founding thesis and the gap nothing surveyed occupies; a command, its streams, an optional port, files on disk. The **one-afternoon spike** (start a dev server, proxy, iframe, kill cleanly) has still never been run. *(DESIGN.md; V0.md spike; PRIOR-ART.md)*
- **The React ladder** — rung 0: iframe a Vite `dist/` via `/f/` (works today) → precompiled custom elements, vendored → pane takeover (a props flag) → artifacts parity via SWC embedded in the daemon. Climb only as far as proves necessary. *(HANDOFF)*
- **html blocks as Vue islands** — the author's reframe (2026-08-08): vendored Vue at `/assets/vendor/` is importable by any srcdoc block — artifact-grade interactive blocks, no CDN, files-in-repo persistence, envelope already sizing/theming. Vendoring + CORS shipped; what remains is leaning in (skill guidance, worked examples). *(HANDOFF)*
- **Plugin architecture** — custom elements + a small explicit `window.sideview` API; shadow-DOM isolation; framework-agnostic; the point where "code you didn't write" starts existing. *(HANDOFF; DESIGN.md isolation)*
- **The extension layer's first residents** — established libraries from CDN, SRI-pinned; extensions may degrade offline, core never does. Mermaid was removed from core to become the first. *(HANDOFF, 2026-08-08)*
- **Pane takeover** — one block filling the viewport below the header; just a session prop. *(HANDOFF ladder)*
</sv-prose>

<sv-prose id="agent-interface">
## The agent interface

- **An MCP route** — `/mcp` as a fourth face of the same binary; tools 1:1 with CLI verbs through the same internals; `watch` becomes an await_comments round-trip; page targeting explicit. Buys hosted no-shell agents over the tailnet. Two invariants: the CLI stays primary, and `/mcp` is the first network *authoring* channel — gated, never a default. *(V2.sv candidates, reopened 2026-08-07)*
- **`--project` flag / louder store identity in CLI output** — store-from-cwd is elegant for humans, hazardous for agents whose shells reset; two live cross-project misfires, one saved only by a foreign key. *(V2.sv candidates, from the agent's own ergonomics report)*
</sv-prose>

<sv-prose id="viewer-chrome">
## Viewer and chrome

- **Chip ordering** — `order` on `<sv-page>` (canon) + viewer drag (localStorage); the standard precedence stack. *(V2.sv candidates; V1.md)*
- **Resizable side rails** — drag-to-resize the contents rail and the comment bar, widths remembered as viewer preference; filed for v3 from the desktop pass. *(V2.sv candidates, 2026-08-09)*
- **Mobile rail** — carried since v0. *(V2.sv candidates)*
- **Outline tabs × explicit outline spec** — explicit outlines assume scrollspy; tabs+spec currently degrades to all-visible. *(HANDOFF gaps)*
- **The `sv-` class layer** — metric/delta, option cards, decision matrix; derive from real plans, keep it guessable. *(V0.md design system)*
- **Unknown-class / `style=` logging** — the measurement of where the vocabulary fails; scraper is in the tree, the TODO sits in render.rs. *(V0.md; HANDOFF)*
- **Electron / desktop packaging** — a window that doesn't get lost among thirty tabs; tray, always-on-top; packaging over an unchanged daemon. *(DESIGN.md)*
</sv-prose>

<sv-prose id="trust-sharing">
## Trust, sharing, provenance

- **T0 — share the snapshot** — frozen HTML, every block as its last output; must be *loud*: a disclosure report of what's embedded, exclude-from-snapshot marks. *(SHARING.md)*
- **T1 — pack and ship** — page files + db + referenced data (`git clone` is the simplest pack); recipient's machine, recipient's authority; with provenance it becomes verification, not just transfer. *(SHARING.md)*
- **T2 — live read-only over the tailnet** — `tailscale serve`, `Tailscale-User-Login` gives comments real authors; read-only enforced server-side by role (Voila's rule). *(SHARING.md)*
- **T3 — constrained interactive** — author-enumerated params, bound never interpolated, read-only handles on scratch copies, caps, audited runs. A different product from a plan; deliberately last. *(SHARING.md)*
- **Provenance** — commit + working diff + input hashes per output; grades `verifiable`/`dated`/`unverifiable`; staleness detection; `sideview verify`. Diff excluded from shares by default. *(DESIGN.md)*
- **Typed, enumerated params from the start** — the cheap pre-decision that keeps T2/T3 possible. *(SHARING.md recommendation)*
- **The auth trigger** — a tailnet node you don't control → `--bind loopback` + `tailscale serve`, better than any token. *(V0.md remote)*
- **`tailscale serve` SSE buffering check** — ten minutes; gates the whole proxied-remote story. Standing since v0, never run. *(HANDOFF)*
</sv-prose>

<sv-prose id="experiments">
## Experiments never run

- **The service-block spike** — the only experiment that could reshape the roadmap (above).
- **An hour with Wave Terminal** — feel `wsh` driving graphical blocks from a shell. *(HANDOFF)*
- **The marimo baseline day** — how badly does a code-cell-shaped, prose-second document read as a plan? Either answer redirects effort. *(PRIOR-ART.md)*
- **Read Livebook's smart-cell contract** before designing the block registry. *(PRIOR-ART.md)*
</sv-prose>

<sv-prose id="housekeeping">
## Housekeeping

- **Reconcile DESIGN.md's marked-stale sections** — each carries a superseded note in place; fold them properly. *(HANDOFF)*
- ~~**Skill drift / `skill install` panic**~~ — fixed in 0.2.1 (2026-08-09, same day): the global `--project <dir>` collided with `skill install --project` (bool); the subcommand flag is now `--repo`, a debug_assert + parse test pins it, and all four harnesses are re-synced.
</sv-prose>

</sv-page>

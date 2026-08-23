<sv-page label="ideas: the continuing pool" category="plan" order="2">

<sv-prose id="intro">
# The pool: everything discussed, still unbuilt

Every idea still standing after four shipped versions, in one place — the continuing pool, no longer tied to any version's curation. [VISION.sv](VISION.sv) holds the north-star this pool is ranked against. v4 drew its In list from here (2026-08-20, through the vision grill); v5 deliberately drew nothing — it is structural work, curated 2026-08-23 through its own simplification grill ([V5.sv](V5.sv)). One line per idea; the reasoning lives in the named doc, and reopening anything deferred twice stays a deliberate act (V2's rule). Comment on a bit to argue it in or out; survivors move to a version's In list at each curation.
</sv-prose>

<sv-prose id="feedback-finish">
## Finishing the feedback loop (v2's honest gaps)

- **Per-line diff comments** (`l:` anchors): line fingerprint plus two lines of context, three-state re-resolution (exact / fuzzy-with-marker / orphan); design complete and watched-ready, unimplemented. Repriced 2026-08-09: selection quotes already survive inside diffs, so this buys precise jump-back, not comprehension. **But it is a prerequisite, not a nicety, the moment a diff is watched** (see the changeset-sources entry: on re-rendering content, quote anchors orphan constantly). *(V2.sv feedback; V1.md)*
- **Per-line comments inside any code block**: the `l:` machinery generalized to any pre. Deferred twice, passed over again at v3's curation. *(V2.sv candidates)*
- **Agent identity in comments**: every harness writes `author: "agent"`, so watchers cannot tell their own echoes from another harness's replies — found live 2026-08-22 when Claude's `--skip-author agent` silently dropped opencode's reply on the very thread testing opencode's watcher (thread 107). The moment two harnesses watch one project (round 22's bar), the label needs a harness dimension (`agent:opencode`) or a separate field; the page's filled-bubble "agent spoke last" and the machine-mail filters would all sharpen with it. *(thread 107)*
- **Rust twin of `anchorHash`**: paragraph hashing is JS-only (FNV-1a 64/48, vector pinned in app.js); the daemon side belongs with diff re-resolution. *(HANDOFF gaps)*
- **sv-note placed physically at its anchor**: today it renders in place with a reference line. *(HANDOFF gaps)*
- **"What changed" on a thread**: after a comment changes the plan, show the block's diff between comment-time and now, on the thread card — the evidence behind the "§ changed (usually: addressed)" badge and V2's designed "edited since commented" marker. Plumbing is mostly in place: the reparse-diff loop holds old and new source at the moment of change; a capped `block_revisions` table in the db (derived state, dies with the db, per the placement principle); render through the existing sv-diff pass. Prose and markup/html sources; iframes sit out. Composes with v4's two-tier editing: an edit request goes in, the agent merges, "what changed" checks the merge. *(chat, 2026-08-20, from the plannotator comparison)*
</sv-prose>

<sv-prose id="live-content">
## Live content

- **Changesets as page sources, and the order the three steps must go in** (author, 2026-08-10, from the crit comparison). The formats work that shipped in v3 generalizes: a page's source is `.sv` composed, `.md`/`.html` imported, and, as the extension, a **patch or a changeset**. Crit worked up from diff review to general commenting while sideview came down from authored plans; the meeting point is a commentable surface over content of several kinds, which is a *format* question, not a feature.
  1. **`.diff` / `.patch` as an imported format**: one `sv-diff` block, ~5 lines in the format dispatch. Cheap and immediately useful: selection comments already work inside diffs, so `sideview open fix.patch` is a review surface with no authored page. Static content, so today's quote anchors are enough.
  2. **`l:` line anchors** (above) must land *before* the watched form, not after. A watched changeset re-renders on every commit and save, and quote anchors would orphan on each one: you would watch your comments turn into "§ changed" every time you typed. The two were designed together in V2 and shouldn't be separated now.
  3. **Watched changesets**: `source = "git:main..HEAD"` in `.sideview.toml`, re-diffed on the poll tick. This bends "pages are files" deliberately and defensibly: the invariant was that canon never lives in the db, and a revspec is a reference whose content is in the object store. Reference-never-embed applied to a page's source. A changeset cannot declare its own label or category, which is exactly what config is for.
  - **Steal from crit while building it: round-to-round.** Two watched sources on one page, `base..HEAD` beside `last-reviewed..HEAD`, make "what the agent did in response" a block rather than a feature. Not stolen, deliberately: the approval gate and unresolved-count (conversations, not tickets), and hosted sharing (SHARING.md declined that fork). Still unmatched by any of this: crit's live-proxy of a running app. That stays the service-block question, and the spike nobody has run.
- **Watched diffs**: `src="git:…"` re-diffed on the poll tick; **gitoxide, never a git subprocess** (attributes/textconv = code execution); constrained revspec grammar. Proven possible, re-sequenced out of v2 and passed over again at v3; step 3 above is its page-level form. *(V1.md diff section; V2.sv candidates)*
- **Sidecar log tailing**: a block names a file the daemon tails; any process (`pytest >> …`, CI, a build) can feed a plan with no agent in the loop. The simplest form arrived in v3 (`sv-csv` re-renders when its file changes); the tailing case, following a file that grows, remains. *(DESIGN.md on-disk)*
- **Syntax highlighting inside diff lines**: syntect is already in the render pass. *(V1.md)*
</sv-prose>

<sv-prose id="blocks">
## Blocks, and the table's missing tier

- **The sqlnow table tier**: the full table, formatted cells and all, exec-per-request through `call_cli`, built by the author's agent against EXTENSIONS.md alone. Deferred past 0.3.0 with sideview's half ready and the contract published; it is also the standing test of whether the contract stands alone. *(V3.sv table trail; HANDOFF)*
- **sv-tree**: a nested markdown list *is* a tree. comrak parses it, CSS draws connector elbows, no layout engine; degrades to an indented list on old binaries. Sequence diagrams the plausible second resident; real DAG layout stays extension territory. *(V2.sv candidates)*
- **Chart, params form, query-plan viewer**: the tier-2 wishlist, never scoped; choose-between-options left it for the ask block above. *(DESIGN.md blocks)*
- **An image/figure block returning**: styled `<figure>` + alt handling; re-read V0's `show` front-door argument before reinventing the command. *(V0.md Out)*
- **Terminal/PTY block**: where websockets finally earn a place; never shared, at any tier. *(DESIGN.md; SHARING.md)*
- **minijinja for structure-heavy block HTML**: assessed suitable, not urgent (V5 thread 120, 2026-08-23). If a Rust template layer is ever adopted it's minijinja over maud/askama — Jinja is the most model-known template vocabulary (the Bootstrap principle applied to templates), pure Rust, zero deps, auto-escape with `|safe` matching the escaped-except-markup model; cost is runtime template errors, mitigated by startup registration. Convert only structure-heavy renderers (page shell, ask forms, csv wrapper) — diff.rs stays hand-built, its logic *is* the code. Trigger: the next block type with a real UI surface. *(V5 thread 120)*
</sv-prose>

<sv-prose id="apps-extensions">
## Apps, services, extensions

- **Service blocks**, supervise → endpoint: the founding thesis and the gap nothing surveyed occupies; a command, its streams, an optional port, files on disk. The **one-afternoon spike** (start a dev server, proxy, iframe, kill cleanly) has still never been run. *(DESIGN.md; V0.md spike; PRIOR-ART.md)*
- **The extension modes not built**: `inline` (renders into the page, inherits the chrome, commentable for free) and `sandbox` (opaque origin for foreign bundles, reserved on purpose); `frame` shipped in v3. *(V3.sv plugins; EXTENSIONS.md)*
- **The extension comment API**: `create_comment` / `get_comments` / `on_comments` and the `c:` anchor form; specced in EXTENSIONS.md, unbuilt. It is the only road to cell comments, since a canvas grid gives the host no text to hash: the app offers commenting, the host never scrapes. *(V3.sv)*
- **Chrome plugins**: rail lenses, header controls, the `window.sideview` API; the only extension kind that can break the page. Named in v3's design, not built. *(V3.sv plugins)*
- **Rung 4, the persistent stdio child**: LSP proper, a warm connection for repeated queries. The ladder rule guards it: argue your way out of exec-per-request on a demonstrated failure (a held connection, state genuinely expensive to rebuild, push rather than pull), never in. *(V3.sv plugins)*
- **CDN residents and distribution**: established libraries from CDN, SRI-pinned; extensions may degrade offline, core never does. Mermaid left core 2026-08-08 to become the first resident and still awaits it. Distribution from outside the repo (npm, crates, a URL) stays its own decision. *(HANDOFF; V3.sv plugins)*
- **How a shipped default extension installs**: embedded-and-implicit (in the binary, config can disable) versus materialized into `extensions/` by a command (contract purity, one more step). Open, deliberately the author's. *(V3.sv table trail)*
- **The React ladder**: rung 0 iframes a Vite `dist/` via `/f/` (works today) → precompiled custom elements, vendored → pane takeover (a props flag) → artifacts parity via SWC embedded in the daemon. Climb only as far as proves necessary. *(HANDOFF)*
- **Pane takeover**: one block filling the viewport below the header; just a session prop. *(HANDOFF ladder)*
</sv-prose>

<sv-prose id="agent-interface">
## The agent interface

- **An MCP route**: `/mcp` as a fourth face of the same binary; tools 1:1 with CLI verbs through the same internals; `watch` becomes an await_comments round-trip; page targeting explicit. Buys hosted no-shell agents over the tailnet. Two invariants: the CLI stays primary, and `/mcp` is the first network *authoring* channel, gated, never a default. *(V2.sv candidates, reopened 2026-08-07)*
- **A wayfinder ritual skill**: mattpocock/skills' wayfinder (multi-session planning as a map of decision tickets on an issue tracker, MIT) adapted to sideview — the map a committed page (destination and decisions canon, arguing in threads), grilling tickets as ask rounds, research tickets as dispatched agents landing blocks, prototypes embedded live, ticket claiming needs a rethink: it was to ride `watch --claim`'s exactly-once semantics, but v5 drops `--claim` unused (the author's read: actual orchestration beats a blocking queue), so claiming would be the orchestrator's job, not the watch protocol's. Route: personal skill first, run on a genuinely oversized chunk of work (v5, a GEM project); ships as the second ritual skill only after `sideview-grill` proves the multi-skill install. *(chat, 2026-08-20)*
- **One agent per page, enforced**: the one-author law is convention today — the CLI targets your own bound page, but nothing *refuses* a write to a page bound to another session. The author's n>1 aim (V5 thread 117, 2026-08-23): n>1 is commenters, never authors; an effective lock, one agent per page. v5's shared edit module is the natural enforcement point, since every splice — CLI and daemon edit-from-page alike — passes through it; direct file edits stay ungovernable (it's the filesystem), which is acceptable because agents following the skill go through the CLI. Browser prose edits are the exception by design (users, not agents; concurrent users last-write-wins after the hash-guard warning; a CRDT for that is far-future and explicitly not an agent concern). *(V5 thread 117)*
- **`update` keeps the block's type**: `sideview update --type` defaults to `prose`, so updating a markup block without the flag silently re-types it and the HTML renders as indented code blocks. Caught live on the vision grill's logo block, 2026-08-19, by the author seeing pre blocks where cards should be. The default should be the block's current type; `--type` stays as the explicit conversion. A daily-driver papercut, primary-adjacent. *(grill page, thread 57)*
</sv-prose>

<sv-prose id="viewer-chrome">
## Viewer and chrome

- **Chip drag-ordering**: viewer drag (localStorage) above the canon `order` that already ships; the standard precedence stack. *(V2.sv candidates; V1.md)*
- **Local search on the index** (author, 2026-08-10): one filter box on `/home`, matching **both categories and pages**. Type "diff" and you get the category if it matches and any page whose label or path does. Client-side, no endpoint; the index already holds every page. Filed against a trigger rather than a date: build it when the lists actually get long, since a search box over eleven pages is furniture.
- **Mobile rail**: carried since v0; also the agreed trigger for moving the contents rail's rendering to Vue (2026-08-10, reasoning in HANDOFF). *(V2.sv candidates)*
- **Chip strip: armed delete survives a repaint.** The two-step ✕ keeps its armed state in a closure, so a sessions snapshot landing mid-arm silently disarms it. Three lines, no framework. *(found 2026-08-10 while measuring the Vue question)*
- **CodeMirror in the compose box**: vim motions as an off-by-default viewer preference, and inline WYSIWYG for comment markdown. The credible route stays a maintainer-built vendored bundle (rung-1 precedent), priced at a few hundred KB; v3's compose pass ended with the plain textarea holding, so this waits for its limits to be felt, not guessed. *(V3.sv attachments)*
- **Outline tabs × explicit outline spec**: explicit outlines assume scrollspy; tabs+spec currently degrades to all-visible. *(HANDOFF gaps)*
- **The `sv-` class layer**: metric/delta, option cards, decision matrix; derive from real plans, keep it guessable. *(V0.md design system)*
- **Unknown-class / `style=` logging**: the measurement of where the vocabulary fails; scraper is in the tree, the TODO sits in render.rs. *(V0.md; HANDOFF)*
- **Electron / desktop packaging**: a window that doesn't get lost among thirty tabs; tray, always-on-top; packaging over an unchanged daemon. *(DESIGN.md)*
</sv-prose>

<sv-prose id="trust-sharing">
## Trust, sharing, provenance

- **`sideview snapshot <file.sv> -o out.html`**: code-driven, one `.sv` in, one self-contained HTML file out: format.rs parse, the existing render pass, then a static shell with CSS and fonts inlined. Repriced cheap 2026-08-09 (thread 27) because rendering is already server-side; extension frames are the new hole, and being loud about them is T0's disclosure rule arriving early. What it drops: liveness, conversation, scrollspy. *(the concrete first step of T0)*
- **T0, share the snapshot**: frozen HTML, every block as its last output; must be *loud* once blocks compute, with a disclosure report of what's embedded and exclude-from-snapshot marks. *(SHARING.md)*
- **T1, pack and ship**: page files + db + referenced data (`git clone` is the simplest pack); recipient's machine, recipient's authority; with provenance it becomes verification, not just transfer. *(SHARING.md)*
- **T2, live read-only over the tailnet**: `tailscale serve`, `Tailscale-User-Login` gives comments real authors; read-only enforced server-side by role (Voila's rule). *(SHARING.md)*
- **T3, constrained interactive**: author-enumerated params, bound never interpolated, read-only handles on scratch copies, caps, audited runs. A different product from a plan; deliberately last. *(SHARING.md)*
- **Provenance**: commit + working diff + input hashes per output; grades `verifiable`/`dated`/`unverifiable`; staleness detection; `sideview verify`. Diff excluded from shares by default. *(DESIGN.md)*
- **Typed, enumerated params from the start**: the cheap pre-decision that keeps T2/T3 possible. *(SHARING.md recommendation)*
- **The auth trigger**: a tailnet node you don't control means `--bind loopback` + `tailscale serve`, better than any token. *(V0.md remote)*
- **`tailscale serve` SSE buffering check**: ten minutes; gates the whole proxied-remote story. Standing since v0, never run. *(HANDOFF)*
</sv-prose>

<sv-prose id="experiments">
## Experiments never run

- **The service-block spike**: the only experiment that could reshape the roadmap (above).
- **An hour with Wave Terminal**: feel `wsh` driving graphical blocks from a shell. *(HANDOFF)*
- **The marimo baseline day**: how badly does a code-cell-shaped, prose-second document read as a plan? Either answer redirects effort. *(PRIOR-ART.md)*
- **Read Livebook's smart-cell contract** before designing the block registry. *(PRIOR-ART.md)*
</sv-prose>

<sv-prose id="housekeeping">
## Housekeeping

- **AUR packaging**: deferred at 0.3.0 (author, 2026-08-16: AUR was down); the binaries exist, the PKGBUILD is the remaining step. *(V3.sv releases)*
- **Reconcile DESIGN.md's marked-stale sections**: each carries a superseded note in place; fold them properly. *(HANDOFF)*</sv-prose>

</sv-page>

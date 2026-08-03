# sideview — design sketch

Working notes on shape, not yet a build plan. See [README](README.md) for what the
project is and why.

## The shape of the thing

Nothing network-shaped between the agent and the server. The agent mutates a **SQLite
database** through a CLI; the server watches it and patches the open page:

```
  agent ──sideview CLI──▶  .sideview/sideview.db  ──▶  server  ─sse─▶  browser
    ▲                                                    │                  │
    └── sideview feedback / SQL reads ───────────────────┘                  │
                                                         └── processes ◀────┘
                                                       (query, CLI, container, PTY)
```

- **The agent authors through a CLI over a local file.** No HTTP, no socket, no library
  import. This is not aesthetic: Claude Code's sandbox blocks localhost daemons and Unix
  sockets — hunk and herdr both hit this — so anything network-shaped forces every agent
  using sideview to run unsandboxed. A CLI writing to a SQLite file works everywhere.
- **The server watches and is long-lived.** It owns the running document: applies changes
  to the open page, runs the processes behind live blocks, and writes derived state back.
  The reader never reloads and never loses the state of a block they were poking at.
- **Feedback comes back through the same store.** Comments, choices and params are rows the
  agent reads with `sideview feedback` or plain SQL. One symmetric channel, one artifact.

## The rule that makes it work: reference, never embed

Everything in this design serves one property — **content reaches the page without passing
through the model's context**. The model names a thing; the daemon fetches and renders it.

That gives one hard rule for every block type: **a spec contains a reference, not the
content.**

```json
{"type": "table",  "sql": "select * from readings where year = 2024"}   ✅  ~15 tokens
{"type": "table",  "rows": [ ... 40000 rows ... ]}                     ❌  the whole point, gone
{"type": "image",  "file": "shots/before.png"}                         ✅
{"type": "log",    "file": "blocks/03-explain.txt"}                    ✅
```

Every block type must be designed so that its expensive part is on the far side of a
reference — a path, a query, a command, a port. Prose is the one legitimate exception,
because prose is the part the model actually authored.

This rule is also the honest test of a proposed block type: if the agent has to read the data
in order to display the data, the block is designed wrong.

## Two surfaces, one machinery

**The scratch stream** — a per-project visual scrollback. `sideview show <thing>` appends a
view and it appears immediately: a parquet file, an image, a diff, a command's output. No
plan, no title, no ordering decisions. This is the visual equivalent of `less`, it's the
thing that earns daily use, and its ergonomics matter more than anything else in the CLI.
One short command, sensible type detection from the argument, no setup.

**Plans** — named, ordered, durable documents with prose between the views. A plan is a
curated scratch stream that someone bothered to write around.

Same blocks, same daemon, same renderer, same store; a plan is just a row in `plans` with an
ordering that someone chose. Building the scratch stream first is also the cheapest possible
path to a working end-to-end system, since it needs no authoring model at all.

## Scope: session, project, daemon are three different levels

"Per project" and "per session" were competing for one slot. They belong at different levels,
and separating them makes both work.

**A session is the scope of a scratch stream, and the owner of what it started.** This is the
right home for ephemeral views: `sideview show foo.parquet` belongs to what you're doing right
now, not to the project forever. It matters most because **concurrent agents are normal here**
— two agents in one project would otherwise interleave their views into a single incoherent
page. A session also owns any processes its service blocks started, so ending a session reaps
its dev servers with no bookkeeping.

Sessions also make worktree-scoping unnecessary. Parallel work is already separated by
session, so there's no need for each worktree to carry its own store — and a worktree's
`.sideview/` should resolve to the main checkout's, so plans don't scatter across ephemeral
directories.

**A project is the scope of the store.** One `.sideview/sideview.db` per project root, found
by walking up like `.git`, holding many sessions. Keeping the *file* at project level is what
preserves the things that made project scope attractive: blocks can use relative paths, `pack`
can bundle a project's plans and data, the store sits next to the code it describes, and
there's one file to open in DuckDB when something looks wrong. A machine-wide database would
lose all four.

**A daemon is per project, started once by the human.** It hosts that project's store and serves
every session in it, each at its own URL (`/s/<session>`), with `/` a live shell that lists them
and switches between them — so one tab stays useful indefinitely and new sessions appear in it
rather than stranding you on a URL only the agent knows.

It's started by hand because of the sandbox: spawning a listener the browser can reach is the one
operation an agent cannot do, so lifting it out of the agent's hands means the CLI never prompts
for anything. Liveness is a `last_seen` timestamp the daemon refreshes in the store, because from
inside the sandbox neither `connect()` nor a pid check can see a host process.

Two alternatives were tried and dropped. **Per machine** was argued on one stable port — one
`ssh -L`, one bookmark — which dissolves over a tailnet where the host is directly addressable and
no forwarding step exists. **Per session, from a pool of pre-started daemons**, gave structural
single-window isolation but made you decide in advance how many sessions you'd run, and guessing
low returned the prompt the whole scheme existed to avoid. Everything stays inside the project —
nothing under `$HOME`, since the sandbox's write allowlist covers the working directory and not
the home directory. See [V0.md](V0.md) for the details.

Multiple daemons writing one project store is fine: SQLite in WAL mode serializes short
transactions, each daemon only writes rows for its own session, and large outputs spill to files
rather than sitting in the database — which is what keeps those transactions short.

### Retention follows from this, and so does the natural workflow

**Sessions get garbage-collected; plans don't.** A scratch stream is disposable — drop it when
its session ends, or after some days. A plan is durable and appears in the index. That's the
whole retention policy, and it needed no separate decision.

Which surfaces the workflow this actually implies: you show yourself things while working,
then keep the ones that turned out to matter. So `sideview promote <view> --to <plan>` is how
plans get built in practice — not authored from a blank page, but assembled from evidence you
already looked at. That's a better story than the one the design started with.

### Identifying the session

> **Superseded by [V0.md](V0.md)'s "Which session am I?".** The chain below includes the controlling
> tty, which V0 removes with reasons — agent Bash calls have no tty and each is a fresh shell — and
> V0 names the harness variable (`$CLAUDE_CODE_SESSION_ID`) rather than leaving it abstract. Build
> from V0.

The CLI has to resolve a session without the agent remembering an id. A fallback chain, first
match wins: `$SIDEVIEW_SESSION` → the agent harness's own session identifier if exposed →
multiplexer pane (`$TMUX_PANE` and equivalents) → controlling tty → cwd. First call creates
the row and caches the mapping, so later calls in the same session land in the same place.
`--session <name>` overrides for the cases where detection is wrong, and for cron jobs with no
tty at all.

## Browser or Electron

**Browser first.** Zero install, works over a tailnet for free, no packaging, no update
channel, and the daemon has to serve HTTP anyway.

Electron buys one real thing: a window that doesn't get lost among thirty tabs, which
matters for a surface you glance at constantly, plus tray and always-on-top behaviour. That's
a packaging decision on top of an unchanged daemon, so it can wait until the thing is worth
glancing at.

## Store vs. interface

These are separate questions and were tangled together earlier.

**The store is SQLite.** Blocks are rows, not text in a document. That buys four things
that a markdown or YAML spec cannot:

1. **Stable block identity, for free.** Feedback anchors to a block and has to survive the
   agent rewriting the prose around it. A row keeps its id through any edit; a paragraph
   in a text file has nothing durable to anchor to. This was the open question at the end
   of the last round, and it disappears.
2. **Precise, atomic patches.** Appending a block is one INSERT; changing one is one
   UPDATE. The server learns exactly which block changed, so it re-renders and re-runs
   only that block — which also answers most of the re-run-semantics question. With a text
   file you diff-and-guess, and the watcher fires mid-write.
3. **Transactional writes.** Five blocks land at once or not at all; the reader never sees
   half a plan. No debouncing, no atomic-rename dance.
4. **One artifact.** Spec, outputs, run history, comments, snapshots in a single portable
   file, with SQL as the read path for all of it.

**The interface is the CLI**, and its central constraint: **prose must never require SQL
escaping.** Most of a plan is prose, and hand-building `INSERT` statements around
multi-paragraph markdown is exactly the sort of thing that goes subtly wrong. So prose
arrives on stdin:

```bash
sideview block add prose --after b6 <<'EOF'
The current parser drops rows where the unit column is blank. That costs us
about 4% of the 2024 file — see the breakdown below.
EOF

sideview block add table --after b7 --spec '{"sql":"select unit, count(*) from readings group by 1"}'
```

Raw SQL against the db stays available as an escape hatch and for reads, but the agent
should never need it to write a plan.

**There is no bulk import format, because the bulk format is a shell script.** The worry
about chattiness — twenty blocks, twenty tool calls — was misplaced: an agent puts twenty
commands in *one* shell invocation, and a quoted heredoc is a perfect prose container. No
escaping of anything, arbitrary content, and it is a form agents emit fluently:

```bash
sideview plan new ingest-rewrite --title "Rewriting the readings ingest"
sideview block add prose <<'EOF'
The current parser drops rows where the unit column is blank — about 4% of the
2024 file. Two options below; the second is slower but keeps provenance.
EOF
sideview block add table --spec '{"sql":"select unit, count(*) from readings group by 1"}'
sideview block add prose <<'EOF'
Option A rewrites in place...
EOF
```

So there is nothing to design, no parser to write, no escaping to get wrong, and no second
representation of a plan to keep consistent with the first. Every format we might have
invented for this was going to be a worse version of "a list of commands".

## Schema sketch

> **Predates the v0 cut.** This is the long-term shape — plans, outputs, params, provenance — and
> none of it is in v0 except `sessions` and `blocks`. The tables to actually create are in
> [V0.md](V0.md)'s "The v0 schema"; keep this as the target the migrations walk towards.

```sql
sessions(id, label, cwd, detected_from, started_at, ended_at)
plans   (id, slug, title, created_at, token, status, origin_session)
blocks  (id, plan_id, session_id, parent_id, ord, type, spec_json, updated_at)
outputs (block_id, run_id, started_at, status, stdout, value_json,
         provenance_id, output_sha)                                -- server-owned
comments(id, plan_id, block_id, author, body, created_at, read_at)  -- server-owned
params  (plan_id, block_id, key, value)                            -- server-owned

provenance      (id, git_commit, git_dirty, diff_blob, tool_versions_json,
                 grade, captured_at)                               -- content-addressed
prov_inputs     (provenance_id, kind, path_or_uri, sha256, size, fingerprint)
```

`ord` is fractional (or a lexicographic rank string) so inserting between two blocks never
renumbers its neighbours. `parent_id` gives containers their children.

A block belongs to exactly one of a plan or a session: `plan_id` set means it's part of a
durable document, `session_id` set means it's a scratch view. Promoting a view moves it from
the second to the first, which is a single UPDATE.

Outputs can be large, and the reference-never-embed rule applies to the daemon's own writes
too — anything past a size threshold spills to `.sideview/blobs/` with the row holding a path.
That keeps the database small and write transactions short, which is what makes one daemon
serving many sessions uncontended.

## Change notification

**The daemon polls `PRAGMA data_version` a few times a second.** Not the elegant answer, but the
right one: the CLI runs inside the agent's sandbox and so cannot poke the daemon over the network,
and a page-cache read a few times a second costs nothing. It also beats watching the WAL and
trying to infer what moved, needs no inotify, and picks up anyone editing the database by hand.

~100ms of latency, which nobody perceives against a browser paint.

## On disk

```
.sideview/
  sideview.db              # everything: sessions, plans, blocks, outputs, comments
  blobs/                   # spilled outputs too large for the db
  blocks/
    03-explain.txt         # sidecar files a block can stream from
  exports/
    2026-08-02-ingest-rewrite.md
    2026-08-02-ingest-rewrite.snapshot.html
```

`.sideview/` in the project root means a plan can reference the repo's own files and
environment by relative path, and one server serves the whole project.

**Sidecar files stay, and they're a sleeper feature.** Streaming a growing log through
SQL updates would be silly, so a block can name a file the server tails:

```json
{"type": "log", "file": "blocks/03-explain.txt"}
```

Now *any* process can feed a plan — `pytest -q >> .sideview/blocks/03-explain.txt` from a
shell, a CI job, a long build. The agent doesn't have to be in the loop for a block to be
alive.

## Backup and export are different jobs, and neither round-trips

**Backup is CSV.** Machine-written, machine-read, never hand-authored — which is exactly
the case CSV is good at, and the escaping that would make it a bad *authoring* format
doesn't matter when nothing types it by hand. Blocks are rows and CSV is rows, so it is a
near-direct dump of the schema: one file per table, restorable with `.import`, and openable
in a spreadsheet or DuckDB when something has gone wrong and you want to look.

Cover the tables that can't be regenerated — `plans`, `blocks`, `comments` (real human
input) and `params`. `outputs` are derived; include them if it's free, but losing them
costs a re-run. Worth knowing that `sqlite3 .dump` is also one line and restores exactly,
so there's no reason not to have both: CSV to inspect, `.dump` to restore faithfully.

**Export is markdown, one-way.** For pasting into a ticket or an email, and for reading a
plan without a server. Making it re-importable would be real parser work for no benefit
now that the shell script is the input path and CSV is the backup — so it doesn't
round-trip, and shouldn't pretend to.

An app block exports as its **last computed output** — the table as a markdown table, the
terminal as a fenced block — labelled with what produced it. That makes the export
genuinely useful as a record rather than a page full of `[interactive block]` holes, and it
means the markdown export and the HTML snapshot are the same generator at two fidelities:
snapshot keeps the visuals, export keeps the substance.

**Version control is not a goal.** Dropping it is right, and the reason is worth stating
because it doubles as a scoping test: the value of a sideview is concentrated in its app
blocks, and an app block is a live query plus its result — not something a diff can
meaningfully show. What *is* diffable is the prose, which is the least interesting part.
And if a plan is nothing but prose, markdown was already the correct tool and sideview
shouldn't have been involved.

## Ownership

The rule that keeps this from getting messy: **the agent writes `blocks` (via the CLI); the
server writes everything derived** — `outputs`, `comments`, `params`, snapshots. Separate
tables, single writer each, no contention on the interesting rows. SQLite's WAL mode
handles the concurrent-reader case without anyone blocking.

## Provenance

Every block output records the state of the world that produced it: the git commit, the
**working-tree diff against that commit**, hashes of the input data, and tool versions.

The diff is the part that matters most. Agents work almost entirely in uncommitted trees —
the whole review workflow around here is built on `git diff` and untracked files — so a
commit SHA alone identifies almost nothing. Commit plus diff identifies the actual code that
produced the number.

**This recovers the good half of version control.** We dropped git-diffable plans and that
was right; the prose was never the interesting part. But the relationship worth having was
never "the plan is versioned" — it was "**the plan knows which version of the code it is
about**", and that is exactly what this gives, in the direction that carries the value.

Three things fall out, none of which needed designing separately:

**Staleness detection.** Reopen a plan two weeks later and the daemon compares recorded
provenance against the world now. Code moved, or the parquet file was replaced? Those blocks
render as computed-against-state-that-no-longer-exists. Plans rot silently today; this makes
rot visible without re-running anything.

**`sideview verify`.** Re-run every block, hash the outputs, compare against `output_sha`.
Report what still reproduces and what drifted. "Proof of reproduction" becomes a command.

**T1 sharing gets teeth.** A packed plan carries its input hashes, so the recipient's daemon
verifies the bundle is what the author measured and can then re-run and compare. Reproduced
on a second machine is a genuinely strong property for a document whose job is to be
believed.

### Grading, because not everything is hashable

Most real blocks won't be fully reproducible, and pretending otherwise is worse than
admitting it. So each output carries a grade the reader can act on:

- **`verifiable`** — every input is content-addressed (files, a bundled scratch DB). Re-runs
  should match exactly; if they don't, something is wrong.
- **`dated`** — inputs are identified but mutable: a live Postgres, an API. Record what can
  be recorded — connection target, a declared fingerprint query, the timestamp — and be
  explicit that a re-run may legitimately differ.
- **`unverifiable`** — a shell command touching the network, a non-deterministic tool. Say
  so plainly rather than implying rigour that isn't there.

A reader deciding how much weight to put on a number is much better served by an honest
grade than by a hash that quietly covers only some of the inputs.

### Practicalities

- **Content-address the provenance row**: `id = hash(commit, diff_sha, sorted input hashes,
  tool versions)`. Identical world state yields an identical id, so twenty blocks run against
  one tree share one row and one copy of the diff, and comparing two runs is comparing two
  ids.
- **Capture per tree-state, not per block.** Shelling out to `git diff` on every block run is
  wasteful in a large repo; capture once and cache on the index mtime plus `status`.
- **Cache file hashes** on `(path, size, mtime, inode)`. Re-hashing a multi-gigabyte parquet
  on every run is not acceptable; for remote objects record the ETag or version id instead.
- **Degrade gracefully.** Not every project is a git repo. Record that fact rather than
  failing, and grade accordingly.
- **Redact.** The diff is source code, and a recorded command line can contain a credential.
  Don't capture environment variables by default, scrub matched secret patterns, and let a
  block opt out of diff capture. Provenance travels inside snapshots and packs, so this is a
  disclosure surface — see [SHARING.md](SHARING.md).

## Two kinds of block: values and services

The central distinction, and the one that separates sideview from everything in
[PRIOR-ART.md](PRIOR-ART.md).

**Value blocks** — run something, capture what it produced, render it. Table, diff, chart,
log, `EXPLAIN` output. One-shot, cacheable, hashable, snapshot-able. This is what every
notebook and every live-document tool does, and it is *evaluate → value*.

**Service blocks** — supervise a long-lived process and give the page a live channel to it.
The project's dev server proxied into the document, an API you can fire requests at, a
container, a REPL, a PTY. This is *supervise → endpoint*, and nothing surveyed does it:
notebook cells evaluate and return, they don't own a process with a lifetime.

Service blocks are the original thesis — embedding sections of a live project so a plan can
show a working prototype in any language — so the design has to treat them as first-class
rather than as an exotic kind of value block.

### The universal contract

Sideview never embeds a language runtime. Its entire contract with a project is:

> **a command, its streams, an optional port, and files on disk.**

That is what makes "any language" true rather than aspirational. A Django app, a Rust
binary, a Go service, a `psql` session and a docker-compose stack are all the same shape to
the daemon. Anything language-specific — a driver that calls one function, a harness that
exercises one module — is a small script the agent writes *in the project's own language*,
and sideview only ever runs it and renders its streams.

### What service blocks change

- **Ports and proxying.** The daemon allocates a port per service block, supervises the
  child, and gives the page a way to reach it. Mounting an arbitrary web app under a
  subpath is notoriously fiddly — absolute URLs, cookies, websockets — so prefer
  per-block origins (a port or a wildcard localhost subdomain) and iframe by origin, rather
  than rewriting someone's routing.
- **Teardown validates the lifecycle decision.** Agent dies → daemon dies → every supervised
  child dies with it. No orphaned dev servers, no leaked containers. That falls out of the
  daemon model already chosen.
- **Provenance grades differently.** A service block has no stable output to hash, so it is
  `unverifiable` almost by definition. What *can* be captured is its inputs: commit and
  diff, image digest, lockfile hash. Record those and be honest about the rest.
- **Snapshots are a real problem.** A live app cannot be frozen into HTML. The honest answer
  is a headless still capture plus the block's config, clearly marked as a photograph — but
  it means T0 sharing degrades exactly where the plan is most interesting. Worth knowing up
  front rather than discovering at snapshot time.
- **Never shared live.** Same rule as PTY blocks — see [SHARING.md](SHARING.md).

## Blocks

Three tiers:

1. **Primitives** — `prose`, `html`, `image`, and containers (`columns`, `tabs`,
   `collapse`). No process behind them. Containers exist because the most valuable thing a
   plan does is put two things side by side; beyond those three, stay flat and linear.
2. **Built-in apps** — the things every plan wants and no agent should write again: a real
   data table (sort, filter, paginate, 100k+ rows from SQL/CSV/parquet), chart, diff,
   terminal/PTY, log stream, params form, choose-between-options, mermaid, query-plan
   viewer. Written once, properly.
3. **Plugin blocks** — project-local or third-party, registered with the server, same
   contract as the built-ins and no privileged access.

The measure of success is how rarely full-document `html` gets used. Every plan reaching for
it is a missing tier-2 block.

## Writing UI: three rungs, and the point is to climb down

This is the wedge that makes sideview worth using on day one, before any service block
exists. The complaint about today's HTML artifacts is not that they look bad — it's that the
agent writes *the entire document every time*, which is slow to generate and slow to revise.
Cutting that is a bigger immediate win than anything else on the list.

So the `html` capability is a ladder, not one thing:

**Rung 1 — `html`, isolated.** A whole document in a sandboxed iframe. The agent owns
everything: markup, styles, scripts. Total freedom, total cost. This is what artifacts do
today, and it stays as the escape hatch for genuinely bespoke visuals.

**Rung 2 — `markup`, inline.** The block renders *into the host page* and inherits the
plan's typography, spacing, colour and dark mode. The agent writes semantic markup against a
predefined vocabulary of classes, plus a few lines of its own CSS or JS where needed. Most
of the code already exists, so the agent writes twenty lines instead of four hundred.

**Rung 3 — a built-in block.** A declarative spec, no markup at all. Cheapest of all.

The design intent is that agents climb *down* this ladder over time. Whatever they keep
hand-writing at rung 2 is precisely the signal for what should become a rung-3 block — which
turns "how rarely is `html` used" from an aspiration into a measurement.

### What the vocabulary has to cover

The saving is real only if the provided classes match what plans actually contain. From the
examples in the README and the shape of design documents generally, that means: callouts
(risk, assumption, open question, recommendation), option cards for comparing approaches,
decision matrices, before/after pairs, step and phase sequences, annotated code, status
badges, and a metric element for the "3.2s → 0.4s" and "4% of rows" figures that carry most
of a plan's argument. Diagrams and tables are already rung-3 blocks.

Keep it **under about thirty class names, guessable and consistent**. Every design system
that grew past what its users could hold in their head got ignored and hand-rolled around,
and an agent will do the same. Two cheap mechanisms help: `sideview styles` prints the
vocabulary as a one-page cheatsheet so it's pulled on demand rather than sitting in context
forever, and the daemon logs any inline `style=` attribute it renders — a direct measurement
of where the vocabulary is failing.

### Two separate speed wins

**Less to write.** Rung 2 is roughly an order of magnitude fewer tokens than a full document.

**What is written appears immediately.** Because blocks patch individually, a plan fills in
block by block as it is authored, rather than appearing only when the whole artifact is
finished. Perceived latency collapses even where total generation time doesn't, and revising
one block re-renders one block. This is a structural advantage over any tool that renders a
document only once it is complete.

## Isolation

**Rung 1 gets a sandboxed iframe** — a full agent-authored document shouldn't be able to leak
CSS into the page chrome, capture keystrokes meant for another block, or break the document
by being malformed. Height via `ResizeObserver` + `postMessage`, and a narrow channel for
block→host calls.

**Rung 2 gets a shadow root with the design system adopted into it.** This is what makes
inline mode work without giving up the benefits of being inline: no style leakage in either
direction, the plan's stylesheet available inside via `adoptedStyleSheets`, normal
participation in page layout, and none of the iframe's sizing and theming pain. Where shadow
DOM is awkward, scoping the block's CSS to its own subtree (`@scope`, or selector prefixing)
is the simpler fallback.

> **v0 does not do this** — see V0.md's "`markup` renders directly into the page". A shadow root
> stops Pico's element selectors reaching inside unless every sheet is adopted, which fights the
> whole *the page is already styled* promise for a robustness benefit that only matters once blocks
> come from code you didn't write. The right trigger to revisit is plugin blocks.

Note the reason: this is **robustness, not security**. A hand-written block shouldn't be able
to break the page by accident. It is not a boundary against the agent that authored it, and
it isn't trying to be — see below.

**Prose, containers and built-in apps render natively.** They're our code, they need to
participate in layout and typography, and an isolation boundary would cost sizing, theming
and cross-block interaction for nothing.

## Scripts in shared plans: not the threat we first assumed

An earlier draft ruled agent-authored script out of shared plans, on the grounds that
arbitrary JS holding a valid session to a process-running API is a confused-deputy problem.
That reasoning was importing a public-web threat model into a situation that doesn't have
one, and it costs the most valuable block type exactly where it matters.

Three things make it the wrong call:

1. **Same-origin policy already contains most of it.** Block script runs in the plan's
   origin. It cannot reach the viewer's other sessions or sites. Its realistic reach is the
   plan's own content — which the viewer was being shown anyway.
2. **The authority it would borrow is removed server-side.** Read-only sharing is enforced on
   the session's *role* — no execute requests accepted from the front end, Voila's rule — not
   by inspecting block content. A read-only session has no process-running authority for
   script to abuse. Stripping scripts was redundant with a control already in place.
3. **The baseline is the org's own code review.** Colleagues already ship each other arbitrary
   JavaScript through dependencies, internal tools and reviewed front-end code, and
   agent-generated code makes a hostile snippet no harder to slip through there than here.
   Holding a shared plan to a stricter standard than the code path everyone already trusts is
   theatre that buys nothing and costs the feature.

So **rung 1 and rung 2 blocks share live, scripts intact.** Two narrow things survive:

- **Third-party plugin blocks are a different question.** Agent-authored markup in your own
  plan is code you own; a plugin is code you didn't write and didn't review. Keep that
  distinction rather than collapsing it into "all custom blocks are fine".
- **A `connect-src 'self'` CSP on shared pages** closes the accidental-exfiltration path for
  almost nothing, since legitimate blocks get their data from the daemon rather than from
  third-party hosts. Cheap, non-blocking, worth having — but a hygiene measure, not the
  boundary that matters.

## Access and trust

Localhost bind, high-entropy token per plan in the opened URL — the Jupyter model, enough
to stop anyone else on the network opening a plan. Two refinements: HTTPS or a local proxy
so the token isn't readable in flight, and expire it with the plan so no long-lived
credential is left behind. (With per-plan tokens as rows in `plans`, revocation is a
one-liner.)

Execution authority is a smaller problem than it appears: the agent authoring a plan
already has shell access, so an embedded terminal escalates nothing. The real risks are a
reader clicking something with consequences, and a plan outliving the intent behind it.
Both are addressed by disclosure and lifetime — blocks declare what they run before
running it, sources default to read-only and to scratch copies of state, and an idle plan
auto-snapshots rather than staying armed.

## Language: Rust core, runtimes as subprocesses

Rust, and the usual "but Python is easier" argument mostly doesn't apply, because of where
the boundary falls.

**Rust for the core** (CLI, server, block host, PTY, snapshotter). A single static binary
is a real feature for a tool meant to be dropped into any project: no venv, no interpreter
version to match, no resolving against the project's own dependencies. `rusqlite`, `actix-web`
and `duckdb-rs` cover the hard parts, and in-process DuckDB keeps the table block fast on
100k+ rows without shelling out. The CLI-over-SQLite design suits Rust especially well —
one binary is both the CLI and the daemon.

**Language runtimes as subprocesses, not embedded.** A Python block runs in *the project's
own* interpreter and venv — its pandas, its drivers, its models. That's strictly better
than embedding a runtime, because the block needs the project's environment, not the
tool's. So "we need Python for the data work" is satisfied by a boundary we want anyway,
and the same mechanism gives R, Julia, node and shell for free.

Honest caveat: Rust is slower to write, and the browser-side TypeScript is identical
either way. If usable-this-month beats distribution, Python + `uv` (`uvx sideview`) gets
close. I'd still take Rust — the daemon and the drop-in-anywhere property are the product.

## Storage roles

**SQLite** is the document and state store, as above.

**DuckDB** is a query engine offered *to* blocks: point the table block at a parquet file,
a CSV, or SQL and let DuckDB do the work; it can attach the project's Postgres or the
sideview SQLite file too. Poor choice for concurrent state (single-writer), so it does no
bookkeeping.

## Terminal-agnostic by construction

The surface is a page served over HTTP, so it is bound to no terminal, no client app and no
tty. It works identically in kitty, herdr, tmux, a bare SSH session, VS Code's terminal, or a
cron job with no tty at all — and that last case is the clean test of the design: a scheduled
agent at 3am can append views to a plan you read at 9am.

This is the deciding difference from the near-misses in [PRIOR-ART.md](PRIOR-ART.md). Wave
binds the display to its client app, the kitty graphics protocol binds it to a live tty, and
MCP-UI binds it to a supporting chat host. Each integrates the visual surface into something
and inherits that thing's constraints. **A URL inherits nothing**, and the cost of asking
someone to change terminals is far higher than the cost of asking them to open a tab.

## Remote is the normal case

The agent usually runs on a different machine from the eyes — here, on a remote box reached
over Tailscale, inside a persistent herdr session. That's the default assumption, not an edge
case, and it needs no new code because the daemon is an HTTP server on localhost:

- `ssh -L 7777:localhost:7777 remote` — works everywhere, nothing to configure.
- `tailscale serve` — nicer: a stable HTTPS URL plus `Tailscale-User-Login` identity, and
  their own guidance to keep the service bound to localhost is the bind we already chose.

This is also the strongest argument for browser-over-Electron. A desktop app would have to
solve remote access itself, which is exactly the machinery Wave had to build.

## Lifecycle: invisible infrastructure

> **Superseded by [V0.md](V0.md)'s "Starting the daemon".** Three things below are reversed there
> and the reasons are specific, so don't code from this section: the daemon is **per project**, not
> per machine (this section also contradicts DESIGN's own scope section, which says per project);
> there is **no idle exit**, because auto-exit requires auto-restart and a sandboxed agent cannot
> restart anything; and **auto-start is conditional** on the namespace being provably reachable.
> What survives intact is *never parented to a tty*, and the three layers of persistence.

One daemon per machine, per the scope section above. An earlier draft said "one per project,
started by the agent, dying with it", which was ambiguous in a dangerous way — read literally,
an implementation could tie the daemon to the invoking shell's tty, and it would then die on
every SSH disconnect, breaking the persistent multiplexer workflow that is the normal case.

The better model, and cheaper to implement: **auto-start, auto-exit, state on disk.**

- **Any `sideview` command starts the daemon if it isn't running**, and nothing ever asks the
  user to manage it. Same ergonomics as a language server, `gpg-agent`, or the Docker socket.
- **It exits on idle** — no page open and no CLI activity for some minutes. Nothing to reap,
  no orphaned process, and an abandoned plan has no live server behind it, which is the trust
  story we wanted.
- **Never parented to a tty.** `setsid`/`nohup` semantics, so it survives a dropped SSH
  connection and lives as long as the work does, not as long as the connection does.

Persistence then has three layers, and the middle one is stronger than tmux's:

1. **Disconnect** — irrelevant, since the daemon isn't attached to the connection.
2. **Daemon death or reboot** — costs nothing, because every plan, block and last output is in
   SQLite. Restart and it is all exactly as it was. tmux loses everything on reboot; this
   doesn't.
3. **Reopening** — a stable per-project URL, so "show me that plan again" is revisiting a
   bookmark rather than reattaching a session.

Because restarting is free, the daemon's lifetime stops being a design question at all.

**Re-runs are explicit — the agent decides.** No dependency graph, no reactive
invalidation, no `depends:` hints. The agent knows why it changed a block and whether
anything downstream now needs re-running, and it says so. This removes what would have
been the single largest source of accidental complexity (and of surprise process
execution).

## The startup path

The aim in [README](README.md) — indistinguishable from opening an HTML file, no second
command ever — is mostly an engineering constraint on one code path. It's achievable, but
several parts of it fail *softly*, in ways that feel bad rather than break.

**Budget.** Rust binary start is single-digit milliseconds, opening SQLite and checking the
schema version a few more, spawning and waiting for the daemon to listen a few tens. Call it
under 50ms of added latency, and the browser launch — 100ms warm, seconds cold — is identical
to the HTML-file case. The dominant cost isn't ours, which is why the target is reachable.

**The CLI must return immediately.** Fire the browser open and don't wait on it. The agent
should never be stalled by a window.

**Auto-start has to be race-free.** Ten parallel agents invoking `sideview show` at the same
moment must not spawn ten daemons. Try to connect; on refusal take an exclusive lock, re-check,
spawn, and wait on a readiness signal — the daemon writing its port to a file atomically after
`listen()` succeeds. Getting this wrong produces intermittent, maddening failures rather than
clean ones.

> **V0.md specifies the mechanism** — a non-blocking `flock` on `.sideview/spawn.lock` — and drops the
> port file: the daemon claims the daemon row after binding, so the row appearing is already the
> readiness signal. Losers of the lock don't wait at all.

**One window, not one per command.** If a viewer is already connected for this session,
`sideview show` must patch that page and *not* launch anything. The daemon knows whether a
live socket exists, so this is cheap — and it is the whole difference between delightful and
infuriating after the fourth invocation.

**First paint must never wait on computation.** Serve the page skeleton and prose immediately,
then stream block outputs in as they resolve. A slow query should make one block look busy, not
make the window look broken. Page load must not block on running anything.

**Migrations happen in-process.** Check the schema version on open and apply what's needed.
Nobody should ever see "please run `sideview migrate`".

**Remote needs an honest fallback.** On a headless remote box `xdg-open` cannot reach your
browser, so detect it (no `DISPLAY`/`WAYLAND_DISPLAY`, or an SSH session) and print the URL
instead of failing at a launch. This is the case where the persistent-tab model wins outright:
with a forward or `tailscale serve` already in place, the tab you have open simply updates, and
no link needs passing at all.

**Install is one file.** A single static binary, which is the Rust decision paying for itself
— no interpreter, no venv, no `node_modules`, nothing to resolve against the project's own
dependencies.

### Acceptance test

From a machine with no daemon running: `sideview show data.parquet` shows rows in a browser
with no other command typed, and the CLI returns in under 50ms. Run it three more times: the
same window updates, and no new tabs appear.

## Build order

> **Superseded by [V0.md](V0.md).** Service blocks are cut from v0, so this ordering describes the
> version after it. The argument for putting a minimal `service` block early still stands on its own
> terms and is preserved in V0's "The spike that isn't v0" as a throwaway afternoon. Reconcile this
> section properly once v0 ships rather than now.

`html`, then a minimal `service`, then `diff`, then `table`.

Starting with `html` is right even though it's a primitive rather than an app: it exercises
the whole pipeline end to end — CLI writes a row, daemon notices, page patches in place,
iframe sandbox and sizing work — with almost no block-specific machinery.

**A bare-bones `service` block should come second, ahead of the value blocks.** It is the
capability with no prior art, the one the whole premise rests on, and the one most likely to
turn out harder than it looks — supervision, port allocation, proxying, teardown, an app
that misbehaves in an iframe. `diff` and `table` are content rather than architecture and
carry almost no risk of surprise; a service block could invalidate the design. Better to
learn that in week two than in month three. The first version needs to do nothing more than
run one command, allocate a port, iframe it, and kill it cleanly.

## Still open

- Shareability — see [SHARING.md](SHARING.md).
- Prior art — see [PRIOR-ART.md](PRIOR-ART.md).

## Resolved by the SQLite decision

- Anchoring feedback to prose that gets rewritten — rows have ids.
- Which block to re-render on a change — the UPDATE says so.
- Watcher racing a partial write — transactions.
- Two stores for authored vs derived state — one store, separate tables.

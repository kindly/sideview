<sv-page label="v2 — closing the feedback loop">

<sv-prose id="intro">
# v2 — closing the feedback loop

v1 made pages files and shipped 0.1.0. v2 makes the page talk back: the user comments, the agent reacts, and the diffs the work produces stay live. Everything below was designed during v1 with reasons attached — rationale lives in V1.md's committed-to-v2 entries; this file is the working plan, and it is itself the first committed document page.

**Two earned principles govern placement:**

- sv files are canon — version-control-worthy plans; the db holds what should not be versioned (coordination, conversation, visual niceness).
- The page file has one author; everything multi-writer goes through SQLite.
</sv-prose>

<sv-prose id="feedback">
## Feedback: comments from the page

- A `comments` table in the db (multi-writer and bursty — SQLite's job): session, target, text, created_at, `author` (nullable now; SHARING T2's tailnet identity fills it later — one column today versus a backfill), and `seen_at` as a **claim marker**, not a read marker. `page rm` cascades its comments.
- Placement is Sphinx's headerlink model: hover a heading or paragraph, a margin mark appears, click to comment there. Existing comments stay invisible until their anchor is hovered — a faint count-dot is the only tell. Mobile has no hover: faint marks, tap to reveal, its own design pass.
- Anchors: headings by their stable prefixed ids; paragraphs by content hash, computed identically client- and daemon-side. Orphans are a query — targets not among the file's current ids — shown at the page tail, re-anchoring automatically if an id returns.
- The browser writes through a comments-only endpoint: the page talks *about* the document, never *as* it.
- Diffs take per-line comments (user- or agent-initiated, promotable to `sv-note`), anchored by **fingerprint, not position**: line text + two lines of context, quoted into the comment at creation so meaning survives churn. Re-resolution when a diff block updates is three-state — exact follows silently, fuzzy follows wearing an "edited since commented" marker, below confidence orphans rather than guesses. Watched-ready for when live diffs arrive.
</sv-prose>

<sv-prose id="watch">
## `sideview watch` — the agent's await

- A blocking read on the store itself: polls `data_version`, prints events as they arrive; `--timeout N` gives up quietly. **Typed JSON-lines from day one** (`{"type": "comment", …}`) so future event kinds don't break consumers; watch holds its own cursor, and `--claim` uses the supersession pattern (`UPDATE … WHERE seen_at IS NULL RETURNING`) for exactly-once when several agents serve one page.
- Sandbox-compatible (SQLite file access, no network) and daemon-independent — works no matter who started what.
- Gives turn-based agents "present the plan, then wait for the user's reaction". stderr nudges on ordinary commands are garnish; watch is the mechanism.
</sv-prose>

<sv-prose id="annotations">
## Annotations: both homes, split by intent

- One shape, two homes. `sv-note` blocks in the file are document content — written by the page's one author, versioned with the plan, always visible (margin-note styling). Context notes go to the db — the *same* comments table as user feedback, written via `sideview comment`, hover-revealed, gone when the db goes.
- **Visibility communicates status: if you can see it without hovering, it's canon.**
- Promotion bridges them, the system's recurring lifecycle: a context note that earns keeping is rewritten into the file as an `sv-note`.
- For diffs: annotations on frozen content (snapshots) may be canon, versioned beside what they annotate; on moving content (watched diffs, when they arrive) they stay conversation.
</sv-prose>

<sv-prose id="outlines">
## Explicit outlines

- `sideview outline` reads the agent's ordered list (title + anchor, ids and sub-ids) into the db; when present the rail uses it verbatim, inference off.
- Prose derivation stays the default — a `##` in the canonical file is structure, not inference. Markup and html contribute nothing without an explicit outline (true since late v1).
</sv-prose>

<sv-prose id="pages">
## Document pages, properly — and the words get fixed

- **Terminology (author, 2026-08-07): page is the noun everywhere.** Chips are pages; the CLI grows `page set / page rm / open <file>`; the db's `sessions` table becomes `bindings`. *Session* survives only as the agent-side routing key (`--session`, the env chain) — how a CLI invocation finds its page, nothing more. A breaking rename, priced correctly at 0.2.
- Nothing about storage changes: all pages are files already, and the db only holds the binding (the daemon's watch list). Committed pages work today — this very file is served through the ordinary machinery — but its binding had to be a hand-written SQL INSERT. The feature is the missing verb.
- Fresh-clone rediscovery: a startup scan re-finds committed `.sv` files. **Chip order for rediscovered pages comes from canon, not scan time**: path order by default, the `order` attribute on `<sv-page>` when the author cares — for committed pages, ordering is plan-worthy, so it lives in the file. Throwaway pages keep creation order.
- `page promote <dest>`: mv a throwaway page into the repo with the binding following.
- iframe autosizing done properly: ResizeObserver + postMessage, retiring the 85vh interim — as a small **versioned envelope** (`{sv: 1, type: "size" | "theme" | …}`), because theme (today's known gap), React blocks and origin-iframed service blocks all want the same channel; theme rides along in v2.
</sv-prose>

<sv-prose id="models">
## The v2 models — for review before a line is written

**Migration v2** (the rename plus the two new tables):

```sql
ALTER TABLE sessions RENAME TO bindings;          -- pages are the noun; this row was always a binding

CREATE TABLE comments (
    id         INTEGER PRIMARY KEY,
    page       TEXT NOT NULL,      -- binding id
    target     TEXT NOT NULL,      -- block id ("b7")
    anchor     TEXT,               -- sub-block position; NULL = the block's tail
    quote      TEXT,               -- the text commented on, captured at creation:
    context    TEXT,               --   quote + surrounding lines are what re-resolution
                                   --   matches against, and meaning outlives placement
    body       TEXT NOT NULL,
    author     TEXT,               -- NULL locally; tailnet identity fills it at T2
    created_at INTEGER NOT NULL,
    seen_at    INTEGER,            -- claim marker, not a read marker
    seen_by    TEXT                -- which watcher claimed it
);
CREATE INDEX comments_by_page ON comments(page, created_at);

CREATE TABLE outlines (
    page       TEXT PRIMARY KEY,
    spec       TEXT NOT NULL,      -- the JSON below, used verbatim by the rail
    updated_at INTEGER NOT NULL
);
```

**Anchor strings** — compact, one grammar for db comments and `sv-note` attrs alike (attr values cannot hold quotes, so anchors are strings, with `quote`/`context` columns carrying the prose):

```text
(absent)              the block's tail — the common case
h:b3-overview         a heading, by its stable prefixed id
p:3f9c2a1b04d2        a paragraph, by 12-hex content hash
l:src/store.rs:8a1f   a diff line: file + fingerprint hash; context column holds the lines
```

**`sv-note`** (in-file annotation, canon):

```text
 <sv-note target="b3" at="l:src/store.rs:8a1f">
 Markdown body — rendered as an always-visible margin note at the anchor.
 </sv-note>
```

**Comment endpoint and watch events**:

```json
POST /api/comments
{"page": "v2", "target": "b3", "anchor": "p:3f9c2a1b04d2",
 "quote": "the paragraph text…", "body": "yay complete"}
```

```json
{"type": "comment", "id": 42, "page": "v2", "target": "b3",
 "anchor": "p:3f9c2a1b04d2", "quote": "…", "body": "yay complete",
 "author": null, "created_at": 1786400000000}
```

— one JSON object per line on stdout; `watch` starts from its invocation moment (`--since <id>` to reach back), `--claim` marks `seen_at`/`seen_by` via `UPDATE … WHERE seen_at IS NULL RETURNING` so concurrent watchers get exactly-once.

**CLI surface** (new and renamed):

```text
sideview comment <block> [--at <anchor>] [--page <id>]   # body on stdin
sideview watch [--timeout N] [--since ID] [--claim]
sideview outline [--clear] [--page <id>]                 # entries on stdin
sideview open <file>                                     # bind a committed page
sideview page set|rm|promote                             # session set/rm live as aliases one release
```

**Outline spec** (stdin and stored form):

```json
[{"title": "Overview", "anchor": "h:b2-overview",
  "children": [{"title": "The store", "anchor": "h:b2-store"}]}]
```

**The iframe envelope** (autosizing + theme, one protocol):

```json
{"sv": 1, "type": "size", "height": 842}
{"sv": 1, "type": "theme", "mode": "dark"}
```
</sv-prose>

<sv-prose id="candidates">
## Pushed to later, deliberately

**An MCP route** — reopened by the author (2026-08-07) after the stateless 2026-07-28 spec deleted what the original rejection objected to (stateful stdio servers, handshakes, session ids, per-client lifecycle). The shape when built: the daemon grows `/mcp` as a fourth face of the same binary; tools map 1:1 onto CLI verbs through the same internals and locks; page targeting is an explicit tool parameter (stateless has no session — honest anyway); `watch` becomes a Multi-Round-Trip `await_comments`. Buys hosted no-shell agents over the tailnet, structured args, self-describing tools. Two invariants: the CLI stays primary (it works daemon-down; MCP doesn't), and `/mcp` is the first network *authoring* channel — a deliberate trust-line crossing, gated (loopback/token/tailnet identity), never a default.

**Watched diffs and the git machinery** — re-sequenced out of v2 by the author (2026-08-07): proven possible, mechanism settled (gitoxide, never a subprocess — V1.md), consciously not now. Plus the standing candidates: chip ordering (agent `order` key + viewer drag) · mobile rail · the scroll-feel decision · `tailscale serve` SSE buffering check · unknown-class logging · tables and app subprocesses — deferred twice; reopening them is a deliberate act.
</sv-prose>

<sv-prose id="goal">
## Goal — blessed by the author, 2026-08-07

v2 is done when its own loop signs it off, on this very page:

1. **The resurrection test, run first**: delete the db, restart the daemon, and this page comes back unprompted — rediscovered from canon, chip in canon order, label from the file — with all comments gone, because conversation dies with the db and documents don't.
2. **Then the sign-off**: every section heading in this document carries a comment, written by the author from the browser: "yay complete."
3. **The agent picks each one up through `sideview watch` — without being prompted in chat.**
4. **0.2.0 on crates.io.**

The done-when is the feature: the plan's completion is announced through the machinery the plan describes, the chat's silence is part of the test — and the order matters, since the resurrection test destroys the sign-off's comments.
</sv-prose>

</sv-page>

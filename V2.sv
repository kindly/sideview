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

- **Conversation is two tables, split by what the data is (author, 2026-08-07).** A `threads` table owns *placement* — page, target, anchor, the `quote`/`context` captured at creation, and resolution state — because placement is a property of the conversation, not of each utterance: re-resolution after churn updates one thread row, not N, and a reply written days later never re-captures a quote. A `comments` table owns *utterances*: thread id, body, `author` — **role before identity (author, 2026-08-07): `agent` when written through the CLI, `user` when written from the page**; SHARING T2's tailnet identity later refines the user side only — created_at, and `seen_at` as a **claim marker**, not a read marker. The role also lets a watcher skip the agent's own echoes, and lets the page mark whose turn it is: a thread whose last word is the agent's shows a **filled** bubble — the user's move. Both are multi-writer and bursty — SQLite's job. `page rm` cascades threads, and their comments with them.
- **Threads resolve, never delete (author, 2026-08-07):** `resolved_at`/`resolved_by`, undoable by design — with resolution in the model, per-thread deletion has no job left; conversation still dies only with the db. Resolved and orphaned are the same *kind* of thing — a conversation not attached to a live spot — so one page-tail list serves both. They stay two independent axes: orphaned is *computed* (anchor absent from the file's current ids — never stored, so re-anchoring stays automatic when an id returns), resolved is *stored* (someone decided). Unresolve clears `resolved_at`; the thread reattaches if its anchor still resolves, or sits in the list as a plain orphan. Both correct, no special case.
- **Multiple threads per anchor are allowed** — not for parallel arguments but because threads *succeed* each other: resolve one, and a later concern starts fresh at the same spot. No uniqueness constraint, not even a partial index on open threads: an unresolve that an index can reject is bad UI. "One open thread per bit, usually" is a UI norm — the popover leads with reply-to-the-open-thread and tucks "new thread" behind a smaller affordance — not a schema law.
- Placement (author, 2026-08-07, revising the margin-mark idea after seeing it): the affordance trails the text, never the margin. Every commentable bit ends with a comment bubble — hover-revealed when empty, **persistent with the count inside where a thread lives** (open threads' comments only: resolved conversations live in the tail list, visibly, which keeps the visibility law honest). Click the bubble to comment, or to open the conversation. Mobile has no hover: tap the text to reveal the empty bubble, tap the bubble to comment.
- Anchors: headings by their stable prefixed ids; paragraphs, list items and code blocks by content hash (one `p:` grammar — each is a text bit like any other; loose items defer to the paragraphs inside them, and a code block's bubble floats top-right like any code-block action), computed identically client- and daemon-side.
- The browser writes through a comments-only endpoint: the page talks *about* the document, never *as* it. Replies address the thread id (the popover and watch events both have it in hand); an anchor in the payload only ever places a *first* comment, creating its thread.
- **Who resolves (author, 2026-08-08): whoever's attention the thread holds.** An agent that answers a question replies and stops — the filled bubble is the handoff, and resolving in the same breath teleports the answer to the tail at the exact moment the bubble was pointing at it (observed live, first day of use). Threads are conversations, not tickets. The capability stays symmetric — user-directed cleanup and the sign-off ritual are legitimate agent resolves — so the norm lives in the skill and the CLI help, not the schema.
- Diffs take per-line comments (user- or agent-initiated, promotable to `sv-note`), anchored by **fingerprint, not position**: line text + two lines of context, quoted into the thread at creation so meaning survives churn. Re-resolution when a diff block updates is three-state — exact follows silently, fuzzy follows wearing an "edited since commented" marker, below confidence orphans rather than guesses. Watched-ready for when live diffs arrive.
</sv-prose>

<sv-prose id="watch">
## `sideview watch` — the agent's await

- A blocking read on the store itself: polls a **generation counter the store bumps in the same transaction as every conversation write** (author, 2026-08-08) — one O(1) row read, and the atomicity is the contract: "gen moved" and "the data is visible" are one fact. The lineage, kept so it isn't relitigated: `data_version` was the design until it missed a cross-process commit under WAL after a long idle (live, 2026-08-08 — a comment sat invisible until an unrelated write shook it loose); an aggregate probe stood in briefly; a queue table of undelivered items was considered and rejected — watchers are readers of one shared history (claim already arbitrates who *acts*), not consumers of per-watcher queues. Events print as they arrive; `--timeout N` gives up quietly. **Typed JSON-lines from day one** — `comment`, `resolve`, `unresolve` — so future event kinds don't break consumers; a resolve is an event in its own right, since it is often the "feedback addressed, move on" signal the agent is actually waiting for. Watch holds its own cursor, and `--claim` uses the supersession pattern (`UPDATE … WHERE seen_at IS NULL RETURNING`) for exactly-once when several agents serve one page. A watcher that joins late reaches back with `--since 0 --claim` — the claim marker is what makes reach-back safe, and it should be the standing form (learned live, 2026-08-07: three pre-watcher comments sat invisible until a db query surfaced them).
- Sandbox-compatible (SQLite file access, no network) and daemon-independent — works no matter who started what.
- Gives turn-based agents "present the plan, then wait for the user's reaction". stderr nudges on ordinary commands are garnish; watch is the mechanism.
</sv-prose>

<sv-prose id="annotations">
## Annotations: both homes, split by intent

- One shape, two homes. `sv-note` blocks in the file are document content — written by the page's one author, versioned with the plan, always visible (margin-note styling). Context notes go to the db — the *same* threads-and-comments tables as user feedback, written via `sideview comment`, hover-revealed, gone when the db goes.
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
- **Scroll (author, 2026-08-08 — the standing v0 question, decided on a day of real use): the reading position is sacred.** No auto-follow, ever — genuinely-new content that lands below the fold gets a floating "new content below" pill, offered not imposed. Changes above the viewport are counter-scrolled so the text under the eye stays put (wholesale block replacement defeats the browser's native scroll anchoring, so the client keeps its own reading anchor) — this covers block edits, comment decoration, and iframe sizing, whose last-known heights are also remembered per block so rebuilds start at the right size. Reconnects remember the block being read and restore it once the replay burst goes quiet — and refreshes ride the same machinery through sessionStorage, with the browser's own restoration set to manual (it races the SSE stream and clamps early; observed 2026-08-08). Diagnosis that led here lives on the scroll-feel page.
</sv-prose>

<sv-prose id="models">
## The v2 models — for review before a line is written

**Migration v2** (the rename plus the two new tables):

```sql
ALTER TABLE sessions RENAME TO bindings;          -- pages are the noun; this row was always a binding

CREATE TABLE threads (
    id          INTEGER PRIMARY KEY,
    page        TEXT NOT NULL,     -- binding id
    target      TEXT NOT NULL,     -- block id ("b7")
    anchor      TEXT NOT NULL DEFAULT '',
                                   -- sub-block position; '' = the block's tail.
                                   --   '' and not NULL: NULLs compare distinct in SQL,
                                   --   and tail threads must group like any other anchor
    quote       TEXT,              -- the text commented on, captured at thread creation:
    context     TEXT,              --   quote + surrounding lines are what re-resolution
                                   --   matches against, and meaning outlives placement
    created_at  INTEGER NOT NULL,
    resolved_at INTEGER,           -- NULL = open; resolve is undoable, never delete
    resolved_by TEXT               -- 'agent' | 'user' — who decided
);
CREATE INDEX threads_by_page ON threads(page, resolved_at);
-- deliberately no UNIQUE(page, target, anchor), not even partial on open threads:
-- threads succeed each other at an anchor, and unresolve must never fail on an index

CREATE TABLE comments (
    id         INTEGER PRIMARY KEY,
    thread_id  INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    author     TEXT,               -- 'agent' (CLI) | 'user' (page); tailnet identity
                                   --   refines the user side at T2
    created_at INTEGER NOT NULL,
    seen_at    INTEGER,            -- claim marker, not a read marker
    seen_by    TEXT                -- which watcher claimed it
);
CREATE INDEX comments_by_thread ON comments(thread_id, created_at);

CREATE TABLE outlines (
    page       TEXT PRIMARY KEY,
    spec       TEXT NOT NULL,      -- the JSON below, used verbatim by the rail
    updated_at INTEGER NOT NULL
);
```

**Anchor strings** — compact, one grammar for db comments and `sv-note` attrs alike (attr values cannot hold quotes, so anchors are strings, with `quote`/`context` columns carrying the prose):

```text
(absent / '')         the block's tail — the common case; absent in sv-note
                      attrs, '' in the db column, one meaning
h:b3-overview         a heading, by its stable prefixed id
p:3f9c2a1b04d2        a paragraph, list item or code block, by 12-hex content hash
l:src/store.rs:8a1f   a diff line: file + fingerprint hash; context column holds the lines
```

**`sv-note`** (in-file annotation, canon):

```text
 <sv-note target="b3" at="l:src/store.rs:8a1f">
 Markdown body — rendered as an always-visible margin note at the anchor.
 </sv-note>
```

**Comment endpoints and watch events** — a reply names its thread; an anchor only ever places a first comment, creating its thread:

```json
POST /api/comments
{"page": "v2", "target": "b3", "anchor": "p:3f9c2a1b04d2",
 "quote": "the paragraph text…", "body": "yay complete"}     first comment — creates its thread
{"page": "v2", "thread": 7, "body": "second thoughts…"}      reply

POST /api/threads/7/resolve                                  and …/unresolve; never DELETE
```

```json
{"type": "comment", "id": 42, "thread": 7, "page": "v2", "target": "b3",
 "anchor": "p:3f9c2a1b04d2", "quote": "…", "body": "yay complete",
 "author": "user", "created_at": 1786400000000}
{"type": "resolve", "thread": 7, "page": "v2", "by": "user",
 "created_at": 1786400000000}
```

— one JSON object per line on stdout; `watch` starts from its invocation moment (`--since <id>` to reach back), `--claim` marks `seen_at`/`seen_by` via `UPDATE … WHERE seen_at IS NULL RETURNING` so concurrent watchers get exactly-once.

**CLI surface** (new and renamed):

```text
sideview comment <block> [--at <anchor>] [--page <id>]   # body on stdin; creates a thread
sideview comment --thread <id>                           # reply to an existing thread
sideview resolve <thread> [--undo]                       # the agent's "feedback addressed"
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

**Watched diffs and the git machinery** — re-sequenced out of v2 by the author (2026-08-07): proven possible, mechanism settled (gitoxide, never a subprocess — V1.md), consciously not now. Plus the standing candidates: chip ordering (agent `order` key + viewer drag) · mobile rail · `tailscale serve` SSE buffering check · unknown-class logging · tables and app subprocesses · patch-in-place block rendering (would restore native scroll anchoring; the client-side reading anchor covers the symptom for now) · per-line comments *inside code blocks* (author, 2026-08-07, filed as a comment on the models block itself — the diff `l:` fingerprint machinery generalized to any pre) — deferred twice; reopening them is a deliberate act.
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

<sv-note id="note-loop-live" target="goal">
Partially exercised live, 2026-08-08, ahead of the formal ritual: comment → watch → reply → resolve each ran end to end, and two features (code-block bubbles, author-as-role) were requested *through the page* and shipped the same hour. This note is itself the sv-note demonstration — canon, always visible, versioned with the plan.
</sv-note>

</sv-page>

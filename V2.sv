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

- A `comments` table in the db (multi-writer and bursty — SQLite's job): session, target, text, created_at, seen_at.
- Placement is Sphinx's headerlink model: hover a heading or paragraph, a margin mark appears, click to comment there. Existing comments stay invisible until their anchor is hovered — a faint count-dot is the only tell. Mobile has no hover: faint marks, tap to reveal, its own design pass.
- Anchors: headings by their stable prefixed ids; paragraphs by content hash, computed identically client- and daemon-side. Orphans are a query — targets not among the file's current ids — shown at the page tail, re-anchoring automatically if an id returns.
- The browser writes through a comments-only endpoint: the page talks *about* the document, never *as* it.
- Diffs take per-line comments (user- or agent-initiated, promotable to `sv-note`), anchored by **fingerprint, not position**: line text + two lines of context, quoted into the comment at creation so meaning survives churn. Re-resolution when a diff block updates is three-state — exact follows silently, fuzzy follows wearing an "edited since commented" marker, below confidence orphans rather than guesses. Watched-ready for when live diffs arrive.
</sv-prose>

<sv-prose id="watch">
## `sideview watch` — the agent's await

- A blocking read on the store itself: polls `data_version`, prints comment events as they arrive; `--timeout N` gives up quietly.
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
## Document pages, properly

- Register a committed `.sv` as a page (`sideview open <file>`, name tbd) — the binding this very file needed by hand.
- `session promote <dest>`: mv a throwaway page into the repo with the binding following.
- iframe autosizing done properly: ResizeObserver + postMessage, retiring the 85vh interim.
</sv-prose>

<sv-prose id="candidates">
## Pushed to later, deliberately

**Watched diffs and the git machinery** — re-sequenced out of v2 by the author (2026-08-07): proven possible, mechanism settled (gitoxide, never a subprocess — V1.md), consciously not now. Plus the standing candidates: chip ordering (agent `order` key + viewer drag) · mobile rail · the scroll-feel decision · `tailscale serve` SSE buffering check · unknown-class logging · tables and app subprocesses — deferred twice; reopening them is a deliberate act.
</sv-prose>

<sv-prose id="goal">
## Goal — proposed, awaiting the author

The loop closes: an agent presents a plan and `sideview watch`es; you comment on a paragraph from your phone; the block updates before you've scrolled away. Done-when bars get set when the scope is blessed.
</sv-prose>

</sv-page>

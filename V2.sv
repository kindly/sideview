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
</sv-prose>

<sv-prose id="watch">
## `sideview watch` — the agent's await

- A blocking read on the store itself: polls `data_version`, prints comment events as they arrive; `--timeout N` gives up quietly.
- Sandbox-compatible (SQLite file access, no network) and daemon-independent — works no matter who started what.
- Gives turn-based agents "present the plan, then wait for the user's reaction". stderr nudges on ordinary commands are garnish; watch is the mechanism.
</sv-prose>

<sv-prose id="watched-diffs">
## Watched diffs

- `src="git:HEAD"` on `sv-diff`: the daemon re-diffs on its poll tick — a live view of the working tree in the middle of a document.
- Via gitoxide, never a `git` subprocess (the textconv escalation — reasons in V1.md), with a constrained `git:` revspec grammar.
- Comments on a watched diff use content-fingerprint anchors re-resolved per re-diff; an anchor that leaves the diff renders as "possibly resolved" — on uncommitted work, disappearing usually means addressed.
</sv-prose>

<sv-prose id="annotations">
## Annotations: both homes, split by intent

- One shape, two homes. `sv-note` blocks in the file are document content — written by the page's one author, versioned with the plan, always visible (margin-note styling). Context notes go to the db — the *same* comments table as user feedback, written via `sideview comment`, hover-revealed, gone when the db goes.
- **Visibility communicates status: if you can see it without hovering, it's canon.**
- Promotion bridges them, the system's recurring lifecycle: a context note that earns keeping is rewritten into the file as an `sv-note`.
- For diffs: annotations on moving content (watched) are conversation; on frozen content (snapshots) they may be canon, versioned beside what they annotate.
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
## Candidates, not committed

Chip ordering (agent `order` key + viewer drag) · mobile rail · the scroll-feel decision · `tailscale serve` SSE buffering check · unknown-class logging · tables and app subprocesses — deferred twice; reopening them is a deliberate act.
</sv-prose>

<sv-prose id="goal">
## Goal — proposed, awaiting the author

The loop closes: an agent presents a plan and `sideview watch`es; you comment on a paragraph from your phone; the block updates before you've scrolled away. Done-when bars get set when the scope is blessed.
</sv-prose>

</sv-page>

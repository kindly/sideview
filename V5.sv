<sv-page label="v5: fewer concepts, same features" category="plan" order="3">

<sv-prose id="intro">
# v5: fewer concepts, same features

v4 made the page ask back and shipped as 0.4.0 (2026-08-23; 0.4.1 the same hour). v5 was
curated 2026-08-23 through a simplification grill run on the machinery itself — four rounds,
every decision made at the block. It ships almost no new surface: the work is structural.
Rationale lives on the grill page and in the design docs; this page holds only what v5
commits to.

Carried principles, not up for re-decision: sv files are canon and the db holds what
should not be versioned · the page file has one author and everything multi-writer goes
through SQLite · reference, never embed · dogfood first.

**The v5 measure, set in round 1 of the grill: context needed per change.** Internal
libraries behind narrow interfaces; lower stack depth over smaller modules. Big files were
explicitly *not* the pain, and splitting for splitting's sake is anti-goal. Nothing hurts
yet — this version is preventative.
</sv-prose>

<sv-prose id="in">
## In

- **One noun: `page`.** The `session` vocabulary and the duplicate CLI verbs go — one
  name everywhere, pre-1.0 breakage accepted. Lands first: mechanical, touches everything
  shallowly, and the extracted pieces get born with the right names. *(grill r2/r3)*
- **The block stack becomes an internal crate.** format, render, ask, csv, diff
  (~1,660 lines) behind one narrow interface: parse .sv, render blocks. *(r2)*
- **The poll loop moves out of daemon.rs.** Poll loop + page loading + event builders
  (~470 lines) already run on their own thread and share nothing with actix; daemon.rs
  keeps only what genuinely needs the web framework. *(r2)*
- **The daemon↔cli cycle breaks** — the one dependency cycle in the codebase. The splice
  primitives (`edit_page`, `append_block`, `next_block_id`) move to a shared edit module;
  daemon and cli both call down, never sideways. *(r2)*
- **Conversation becomes an internal library.** Threads, comments, and their operations
  behind one interface; daemon, cli, and monitor become thin callers. Comments are the
  project's biggest feature (~1,500 lines across six files) and its least removable —
  closing the feedback loop is the premise — so the lever is extraction, not removal.
  Attachments ride on comments and fold into this library rather than getting a fourth
  home. *(r3; thread 117)*
- **The layering law**, settled in thread 117, governs every extraction. Three layers, two
  of them physical: **concept modules** hold models fused with their algorithms (format.rs
  is the in-repo proof of the shape; a shared pure-models layer would recreate a hub);
  **one thin logic layer** owns sequencing and transactions, and everything the interfaces
  do passes through it even when the call is a single line — with two interfaces plus the
  monitor, "thin" is where an operation is written once instead of twice (posting a comment
  exists in cli.rs *and* daemon.rs today, with drift: one broadcasts immediately, the other
  waits for the poll tick); **interfaces** (daemon, cli, monitor) only parse, call the
  logic layer, and format for their transport — they import nothing below it, checkable
  from the use lines, and they never decide: an if-statement inspecting a model moves down
  into its concept. Cross-concept needs go boundary-first (fold, as attachments into
  conversation), else one-directional concept-to-concept logic calls (a DAG, like
  format ← render today). Pipelines (block stack, poll loop) stay pipelines — the layering
  is for the CRUD-shaped half, not a costume for everything. *(thread 117)*
- **Watch simplifies to one delivery concept, and gains filters.** `--claim`
  (exactly-once multi-watcher splitting) is dropped: orchestration beats a blocking queue,
  and a claimed-then-crashed event is invisible forever. `--ack` stays — it is the delivery
  receipt behind the reader's "sent" indicator. `watch` gains `--page` and `--category`,
  both repeatable, landing when the conversation library defines its query surface. *(r3/r4)*
- **app.js splits into real ES modules, served as-is — no build step.** A Vue shell
  rewrite was investigated and skipped (r3, reversing r2's spike): the shell is mostly
  imperative browser integration a framework can't help, and the island pattern already
  delivers Vue where state is heavy. Vue stays islands-only; the selection chip folds into
  the comment island during the split — one commenting UI instead of two. The comment bar
  island decomposes into child components with small `/* html */`-annotated template
  literals — readability through decomposition, no compiler: browser-side compiler-sfc,
  Vite, and daemon-side fervid were each weighed and declined (too heavy, the Node
  toolchain, too early — thread 119). *(r3/r4; thread 119)*
- **One fix rides along: blocks morph instead of being replaced.** The single pool item
  pulled into v5 (thread 121). Wholesale innerHTML replacement on every SSE patch is what
  the scroll/reading-position machinery exists to compensate for, and the full-page
  re-render is visible — worst on imported md pages, where one block is the whole page.
  The fix is **idiomorph, vendored** (single file, no deps, MIT — the htmx family's
  DOM morpher): morph the new HTML into the live DOM so unchanged nodes keep their
  scroll, selection, open `<details>`, and same-content iframes stay put. Deliberately
  *not* a framework (that rewrite was weighed and skipped in r3) and *not* websockets —
  transport was never the problem: SSE delivers updates fine; the jank is in how they're
  applied, and V0's not-websockets decision stands. Lands with the JS module split, where
  block rendering becomes its own module and the scroll-preservation module shrinks to
  the "new below" pill and the reconnect anchor. *(thread 121)*
</sv-prose>

<sv-prose id="kept">
## Deliberately not touched

Recorded so they aren't re-proposed as cuts:

- **No feature is removed.** Extensions and netcheck/LAN were flagged as unused in round 1
  and kept anyway in round 2 — first-class, no quarantine.
- **store.rs is never split for splitting's sake** — the r2 decision (a `store/{domain}`
  file split makes smaller files, not fewer concepts) — but thread 117's layering law
  supersedes it in part: concept modules own their SQL, so the conversation and attachments
  queries leave with the conversation library, and store.rs keeps what has no concept
  module — open/migration, bindings, outlines, daemon liveness — still one file.
  **The codex monitor stays in the main crate.**
- **Outline keeps its SQLite home and CLI verb**; `working` and the `h:`/`p:` anchor pair
  stay as they are. Attachments keep every behavior too — their *home* moves into the
  conversation library (thread 117), a structural move, not a cut.
</sv-prose>

<sv-prose id="sequence">
## Order of work

1. The `page` rename — mechanical, touches everything shallowly, and the extracted
   pieces get born with the right names.
2. The conversation library, first among the extractions and pulled forward deliberately
   (thread 117): it's the piece the n>1-commenters experiment leans on. Attachments fold
   in, the watch `--page`/`--category` filters land here, and the logic layer is born
   with it — the first operations written once instead of twice.
3. The remaining extractions: block crate, poll loop out of daemon.rs, the shared edit
   module (which ends the daemon↔cli cycle and is the future one-agent-per-page
   enforcement point, if that pool idea is ever built).
4. The JS module split: ES modules, the comment-bar decomposition, chip-into-island,
   and idiomorph block morphing.
</sv-prose>

</sv-page>

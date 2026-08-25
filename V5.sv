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
- **The block stack becomes one module behind one narrow interface** — format, render,
  ask, csv, diff (~1,660 lines): parse .sv, render blocks. Built as a workspace crate
  first, then deliberately inlined to `src/blocks/` in round 4: a crate useful only to
  this binary would pollute crates.io, so the boundary is convention (nothing in blocks/
  imports models, logic, or interfaces) rather than the compiler, and the crate returns
  only if the project grows real outside users. *(r2; drill r4)*
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
  is for the CRUD-shaped half, not a costume for everything. The layers are directories
  (round 3, executed fc4f8cd): `models/{base,conversation}.rs` and
  `logic/{base,conversation}.rs`, base holding what has no concept of its own —
  layer-first over concept-first deliberately, because cross-cutting logic that owns no
  model (gc, the rm cascade) gets a plain file in logic/ with no models/ twin, which
  concept-first cannot express (thread 126). logic/base is thin one-line wrappers so the
  import law is total; the Store handle itself stays the argument everything passes.
  *(threads 117, 125–127)*
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

<sv-prose id="drill1">
## Drill: the rename (step 1, committed fc89732)

The rename is in: `--page` on every verb, `sideview pages`, the `session` alias gone, the
SSE event and block-event keys renamed, app.js and the skill following, 0.5.0. Four
surprises went to round 1; **settled 2026-08-24 (thread 124)**: `/s/` overruled — page
URLs are `/p/` now, landed as 45f48cf, old bookmarks 404 — and identity.rs, the verbatim
migration DDL, and the clean break all stood as recommended. Step 1 closed; the daemon
needs `sideview restart` to serve 0.5.0.
</sv-prose>

<sv-ask id="d1q1" round="1">
**The `/s/` URL prefix survived.** Every page URL is `/s/<id>` — the one arguably
session-flavored spelling left. Changing it to `/p/` breaks every bookmark and printed URL
for one letter of purity; the letter never says "session" to a reader.
- * Keep `/s/` — it's a letter, not a noun; bookmarks outweigh it
- Rename to `/p/` now, while 0.5.0 is already breaking everything
</sv-ask>

<sv-ask id="d1q2" round="1">
**session.rs became `identity.rs`, not part of a page module.** Its job is resolving
*which page id this invocation writes to* from the harness environment (the env rungs are
genuinely harness-session facts, and its comments still say so). The alternative was
folding it into a future page/bindings concept module during the extractions.
- * identity.rs as landed; fold into a bindings concept module later only if one emerges
- Should have been page_id.rs / folded now — rename again before more code lands on it
</sv-ask>

<sv-ask id="d1q3" round="1">
**The store's migration DDL still says `sessions`.** Migration steps replay history on old
databases (`CREATE TABLE sessions`, `ALTER TABLE sessions RENAME TO bindings`), so those
strings must stay verbatim — the *table* has been `bindings` since v2. Recorded here so a
future grep for "session" doesn't read them as a miss.
- * Correct as landed (nothing to change; this question is the record)
- Squash migrations instead: pre-1.0, `reset` exists, history could restart at v0.5
</sv-ask>

<sv-ask id="d1q4" round="1">
**No grace release for the removals.** `sideview session …` and `/api/sessions/…` are
gone outright in 0.5.0 (the alias's own comment promised one release of grace — v4 was
it). Anything scripted against the old verbs breaks on upgrade with a clap error, and an
un-restarted 0.4.x daemon serves an old client to a 0.5 CLI until `sideview restart`.
- * Clean break as landed — pre-1.0 rules, the skew cure is `sideview restart`
- Re-add a hidden alias for one more release
</sv-ask>

<sv-ask id="d1fin" round="1" role="close">
Drill round 1: the rename's four surprises. Approving closes step 1; step 2 (the
conversation library) starts next.
</sv-ask>


<sv-prose id="drill2">
## Drill: the conversation library (step 2, committed 4c4d061)

conversation.rs (671 lines: models + algorithms + SQL + event shapes) behind ops.rs (343
lines: the logic layer). store.rs shrank 1272 → 647. post_comment, the guards, and the
watch decisions each exist once; daemon, cli, and monitor import ops and nothing below
it. `--claim` is gone; `--page`/`--category` work, verified live against this store.
Round 2 settled all five surprises as recommended (thread 125) — but revised the
*structure*: the flat ops.rs and its name go, replaced by two directories, `models/` and
`logic/`, each categorized by concept with a `base` for what has no concept of its own.
The logic layer gets categories too, not just the models. Round 3 below drills the exact
shape before the move.
</sv-prose>

<sv-ask id="d2q1" round="2">
**The snapshot JSON lives in ops, not conversation.** The plan said the concept module
owns "the one serialization both watch and SSE emit". The watch event shape does live
there — but the SSE snapshot renders comment bodies to HTML via the render module, and
putting it in conversation would have added a conversation→render dependency (wrong
direction: render is the block pipeline). So conversation owns the watch-event shape,
ops composes the snapshot from conversation + render.
- * Right call — cross-concept composition is exactly what the logic layer is for
- Wrong — move snapshot into conversation and accept the render dependency
</sv-ask>

<sv-ask id="d2q2" round="2">
**Fixture exception to the import law.** Production code in daemon/cli/monitor imports
only ops (grep-verified). But their *test modules* call conversation directly to set up
fixtures (create a thread, read back comments). The alternative is routing fixtures
through ops::post_comment, which drags its validation into tests that aren't testing it.
- * Tests may import the concept module; the law governs production imports
- Strict: tests go through ops too, fixtures included
</sv-ask>

<sv-ask id="d2q3" round="2">
**Category resolution reads files inside watch_tick.** `--category` resolves category →
pages by reading each binding's sv-page tag (or config) on every generation change — not
cached, so a page changing category is picked up live, at the cost of a few file reads
per event burst (only when --category is used). The alternative was resolving once at
watch start: cheaper, but a category change mid-watch would be missed.
- * Live resolution per tick — correctness over a negligible cost
- Resolve once at start; a category change means restarting the watcher
</sv-ask>

<sv-ask id="d2q4" round="2">
**The gen counter stayed shared.** Outline writes and the page-rm cascade (store.rs) bump
conversation's generation counter — `bump_gen` is pub(crate) and crossed the module line,
because watchers and the daemon's poll loop key *all* their reloads off that one counter.
The pure alternative (a counter per concept) means every poller polls two counters for no
behavioral gain today.
- * One shared counter, conversation owns it, others may bump — revisit only if a second real poller family appears
- Split the counter per concept now
</sv-ask>

<sv-ask id="d2q5" round="2">
**The pi-watch recipe simplified more than planned.** Its claimSafe machinery existed
only because --page didn't: with the native filter and no claim, the skill's page mode is
one flag, and the "two project-wide claimers" warning became "two watchers both deliver —
one steward per project, or scope by page". The one-agent-per-page pool idea remains the
eventual enforcement.
- * As landed (this question is the record; nothing to decide unless it reads wrong)
- Something reads wrong — rider below
</sv-ask>

<sv-ask id="d2fin" round="2" role="close">
Drill round 2: the conversation library's five surprises. Approving closes step 2; step 3
(block crate, poll loop, edit module) starts next.
</sv-ask>


<sv-prose id="drill3">
## Drill: the models/ and logic/ directories (round 3, before the move)

The target: `models/conversation` (today's conversation.rs — models fused with algorithms
and SQL) and `logic/conversation` (today's ops.rs), beside `models/base` and `logic/base`
for the domains that have no concept of their own. Four questions decide the exact shape.
**Approved all as suggested and executed (fc4f8cd, thread 127)**; the layer-first
rationale — cross-cutting logic needs no model twin — recorded from thread 126.
</sv-prose>

<sv-ask id="d3q1" round="3">
**What is `models/base`, and where does the Store struct live?** store.rs today =
infrastructure (the Store struct, open, migrate, meta) + three small domains (bindings,
outlines, the daemon liveness row).
- * store.rs moves wholesale to `models/base.rs` — infrastructure and the uncategorized domains in one file, least churn, "one file" spirit kept
- Split: store.rs keeps the Store struct/open/migrate as pure infrastructure; only the three domains move to models/base.rs
- Keep store.rs where it is, name unchanged; models/ starts with conversation only
</sv-ask>

<sv-ask id="d3q2" round="3">
**Does `logic/base` get written now?** Interfaces still call bindings/outline/daemon-row
operations on the Store directly — the conversation ops were the logic layer's first
residents. Writing logic/base now (thin wrappers for outline set/clear, binding
list/get/delete, daemon lifecycle) makes the law total: interfaces import logic/ and
nothing else, zero exceptions, one grep. (Holding the Store handle itself — open, .root —
stays direct either way; it's the argument everything passes, not a decision.)
- * Now, thin — the law becomes total while the code is already moving
- Lazily — logic/base grows as each domain is next touched
</sv-ask>

<sv-ask id="d3q3" round="3">
**Naming details.** Two small choices that set the pattern for every future concept.
- * Singular concept names (`models/conversation.rs`, `logic/conversation.rs`), watch machinery inside logic/conversation
- Plural (`models/conversations.rs`) — as written in your note
</sv-ask>

<sv-ask id="d3q4" round="3">
**What stays outside models/ and logic/.** Per the layering law, pipelines aren't the
CRUD shape: the block stack (format, render, ask, csv, diff — step 3's crate), the poll
loop, and the coming edit module stay top-level, as do the interfaces (main, cli, daemon,
monitor) and the small utilities (identity, config, netcheck, skill, ext).
- * Confirmed — models/ and logic/ hold the CRUD-shaped concepts only
- No — pull more of it under the two directories (rider says what)
</sv-ask>

<sv-ask id="d3fin" round="3" role="close">
Round 3: the directory structure, before any file moves. Approving executes the move;
step 3 (block crate, poll loop, edit module) follows on the new layout.
</sv-ask>


<sv-prose id="drill4">
## Drill: the remaining extractions (step 3, commits 236a976 / 665e296 / cc0e7ed)

logic/edit.rs holds the splice primitives and daemon.rs no longer imports cli at all;
poll.rs holds the loop, page loading, and event builders (daemon.rs 2125 → ~1500 with
tests); the block stack sits behind "parse .sv, render blocks". 77 tests green
throughout. Four surprises went to round 4; **settled (thread 129)**: the crate was
overruled — inlined back to `src/blocks/` as b105e28, no workspace, don't pollute
crates.io — and the shim, the two placements, and the dead-code deletion all stood.
The Rust half of v5 is closed; step 4 (the JS split) remains.
</sv-prose>

<sv-ask id="d4q1" round="4">
**The crate boundary has a release cost.** A path dependency can't be published to
crates.io unless the dependency is published too: at 0.5.0's release, sideview-blocks
must become a real public crate (its publish=false comes off), or the workspace inlines
back before tagging.
- * Publish sideview-blocks at release — internal in intent, public in registry, version-locked to the binary
- Inline the crate back before each release (workspace for development only)
</sv-ask>

<sv-ask id="d4q2" round="4">
**The re-import shim.** main.rs re-imports the five block modules at the binary's root,
so every call site still reads `crate::format::parse(...)` — zero churn, but a grep for
"who uses the blocks crate" must know about the shim line. The alternative is rewriting
~60 call sites to `sideview_blocks::` paths, making the boundary visible at each one.
- * Keep the shim — one documented line; the Cargo.toml already declares the boundary
- Rewrite the call sites to explicit sideview_blocks:: paths
</sv-ask>

<sv-ask id="d4q3" round="4">
**Two unplanned placements, recorded.** `open_browser` moved cli→daemon (the cycle's
second edge; daemon owns the "open on ready" behavior, cli calls down, one direction
everywhere). And id percent-encoding moved into the blocks crate (`encode_id`) because
rendered ext frames embed their own URLs — identity::encode now delegates to it.
- * Both as landed (this question is the record)
- One of these reads wrong — rider says which
</sv-ask>

<sv-ask id="d4q4" round="4">
**A dead function died, and took a dependency with it.** fragment_outline had no callers
since markup stopped contributing to the outline (456dd3e, pre-v5) — and it was the last
user of scraper, so the whole dependency left both manifests. Strictly a removal in a
no-removals version, but of unreachable code, not a feature.
- * Right call — dead code holding a live dependency is exactly what a structural version should catch
- Restore it (something planned needs scraper soon)
</sv-ask>

<sv-ask id="d4fin" round="4" role="close">
Drill round 4: step 3's surprises. Approving closes the Rust half of v5; step 4 (the JS
module split: ES modules, comment-bar decomposition, chip-into-island, idiomorph) is last.
</sv-ask>

</sv-page>

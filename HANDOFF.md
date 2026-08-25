# Handoff

Written 2026-08-02, at the end of the design conversation that produced this repo. Revised
2026-08-03 after a review pass over all five documents, immediately before starting to code.
Everything here is either current state or something that exists nowhere else in the docs.

## State

**v5 curated: fewer concepts, same features, 2026-08-23.** Hours after 0.4.1, the author
asked for a grilling on simplification and [V5.sv](V5.sv) came out of it — four sv-ask
rounds on a grill page, confirmed through the page's own machinery (round 4, thread 116).
V4.sv moved to shipped and IDEAS.sv was reframed as the continuing pool (no version in its
name; v5 ships no pool features). The version is structural: the measure is *context needed
per change* (internal libraries behind narrow interfaces; lower stack depth over smaller
modules — big files were explicitly not the pain). In: the `page` rename first (the
`session` noun and duplicate CLI verbs go); the block stack (format, render, ask, csv,
diff) as an internal crate; the poll loop out of daemon.rs; the splice primitives to a
shared edit module, breaking the codebase's one dependency cycle (daemon↔cli);
conversation as an internal library with daemon, cli and monitor as thin callers —
`--claim` drops there (orchestration beats a blocking queue; a claimed-then-crashed event
is invisible forever) while `--ack` stays (it drives the reader's "sent" receipt), and
`watch` gains repeatable `--page`/`--category` filters when that library defines its query
surface; last, app.js splits into real ES modules, no build step, the selection chip
folding into the comment island. A Vue shell rewrite was investigated and skipped — the
shell is mostly imperative browser integration a framework can't help; Vue stays
islands-only. Deliberately kept, recorded on V5.sv so they aren't re-proposed as cuts:
every feature (extensions and netcheck were flagged unused in round 1 and kept anyway),
store.rs as one file, the monitor in the main crate, outline's SQLite home, `working`,
attachments, and the `h:`/`p:` anchor pair. A post-confirmation thread on V5 (117) then
settled **the layering law** for all the extractions — concept modules with models and
algorithms fused, one thin logic layer everything passes through (it owns sequencing and
transactions; with two interfaces plus the monitor, it is where each operation is written
once — posting a comment lives in both cli.rs and daemon.rs today, with drift), interfaces
that only parse/call/format and import nothing below the logic layer — and folded
attachments into the conversation library. Thread 121 then pulled exactly one fix out of
the pool into v5: **idiomorph block morphing** replacing wholesale innerHTML replacement
(the cause the scroll/reading-position machinery compensates for; vendored single file,
not a framework and not websockets — the transport was never the problem), landing with
the JS module split. **Goal blessed 2026-08-24 (thread 123 on V5.sv), the v4 ritual
attached**: each completed piece earns a drill — an sv-ask round over whatever the
implementation did that the plan did not predict. **All four steps are implemented,
2026-08-25**: the page rename with /p/ URLs (drill r1); the conversation concept module
behind the logic layer, --claim dropped, watch --page/--category live-verified (r2); the
models/ and logic/ directories with logic/base making the import law total (r3); the
edit module ending the daemon↔cli cycle, poll.rs, and the block stack — built as a
workspace crate, then deliberately inlined to src/blocks/ (r4: don't pollute crates.io);
and the JS split — fifteen ES modules served as-is with the cache stamp as a path
segment, idiomorph morphing the update path, the comment bar decomposed into components,
the chip folded into the island (drill r5 approved live, 2026-08-25 — the author's
first restart picked up the installed 0.4.1 and the honest hold caught v5's one live
failure, a duplicate blockEl import that killed the module graph; fixed, and
tools/jsgraph-check.mjs now link-checks the graph at maintainer time; all three editing
legs then ran on 0.5.0 and the amended approval itself came through the new client). Two dead-code finds en route: fragment_outline took the scraper
dependency with it; renderAllBlocks had been orphaned since 0.3.1. THIRD-PARTY.md was
stale (mermaid still listed, Vue never listed) and is fixed. Version 0.5.0 throughout;
77 Rust tests green at every step. What remains: the release round on V5.sv.

**0.4.1, 2026-08-23.** The post-release patch, the 0.2.1 tradition upheld: the
author found the edit pencil dead on imported (.md/.html) pages minutes after
0.4.0. Shipped same hour: the sessions event carries a `format` field, the client
withholds the edit verb on non-sv pages (chip and touch corner both), and
`/api/source` + `/api/edit` refuse imported pages server-side — the real hazard,
since parsing an .md as sv would have opened stray-block soup and a save could
have mangled the file. Test-pinned, .md bytes verified untouched. Ordered released
directly by the author.

**v4 shipped: 0.4.0 on crates.io, 2026-08-23.** The release order arrived as round
37's approve on V4.sv (thread 111) and executed within minutes: main pushed, tag
v0.4.0 pushed (release.yml building the binaries), `cargo publish` confirmed. Three
days from blessing to release, 37 drill rounds, every feature and the release
itself ordered through the page. What 0.4.0 carries over 0.3.1: the ask block
(`sv-ask` rounds with suggested answers, whole-round collapse, machine-mail
comments), editing from the page (prose splices with the from-hash guard, edit
requests for everything else, on touch too), `sideview restart` + the `→ project`
identity line on every write, the reviewer as favicon/header mark wearing the
connection state, `_sv_mark_<col>` cell tints with the sqlnow bridge and the
csvdiff shapes, the `sideview-grill` and `sideview-pi-watch` skills (plus the
uninstalled opencode recipe), migration v5 (`comments.kind`), and `sideview
monitor codex` — feedback that wakes the harness on all four legs.

**v4 was done except the publish, 2026-08-23.** The round-22 expansion is met on all
three legs, each proven live: Codex via the product monitor (`sideview monitor
codex`), OpenCode via the personal-plugin recipe (accepted round 35, skill kept
separate and uninstalled by design), Pi via the generated project-local extension
recipe (accepted round 36, shipped as the third embedded skill). Everything is
committed (b0b9651, c01d897, 77c5fcf on top of the monitor commit), 76 tests green,
version 0.4.0, skills current on all four harnesses. **The release round is back on
V4.sv (round 37, block `release2`)**: the author's approve is the order to tag
v0.4.0 and publish to crates.io; nothing is tagged or published before it.

**A Pi watcher recipe skill exists, 2026-08-22.** `skills/sideview-pi-watch/SKILL.md`
now tells agents to generate a project-local `.pi/extensions/sideview-watch.ts`
instead of Sideview shipping a resident Pi package prematurely. It is embedded in
`skill install` beside the main and grill skills. The recipe is explicit about Pi's
lifecycle (`session_start`/`session_shutdown`), JSONL parsing, `pi.sendUserMessage`,
project-wide steward claiming, and the current limit: safe page-scoped claiming needs
a real `sideview watch --page` filter; without it, page filtering can only be an
unclaimed demo. A live attempt with a generated extension found the first recipe gap:
slash commands (`/sideview-watch-start`) are user-operable only; an agent cannot start
or stop its own resident watcher unless the extension also registers model-callable
tools. The recipe now requires `sideview_watch_start`, `sideview_watch_stop`, and
`sideview_watch_status` tools sharing the same helpers as the commands. Static
repository checks passed; no live subagent wake test was run in this harness.

**A real Codex queue monitor is implemented, live, and accepted, 2026-08-22.** Round 32
(thread 104) authorized project modification after the queue prototype
proved that Sideview feedback can start a new turn in this exact ChatGPT desktop
conversation. `sideview monitor codex` is now the product surface: foreground by default,
explicit `--detach`, exact UUID binding (`CODEX_THREAD_ID` convenience or `--thread`),
best-effort compatible Codex binary discovery, durable project-local cursor/delivery JSON,
lifetime lock plus PID start-token ownership, readable queue prompts, agent-echo filtering,
post-success receipts, retry without cursor advance, and `monitor status` / verified
`monitor stop`. Five focused tests bring the full suite to 76 green; an isolated detached
start/status/stop test passed. The shell watcher was retired and the built monitor is now
serving this conversation with the desktop-bundled Codex binary (PID 1069591 after the
final lifecycle-race hardening restart). Round 33 (thread 105) accepted at-least-once
delivery: ambiguous post-acceptance crashes may rarely replay an event rather than silently
lose feedback, and retained pending state remains visible in status. Event 288 was the live
product-monitor acceptance test; it queued into this same desktop conversation, left no
pending event or error, and advanced the durable cursor to 288. The hardening pass is
complete. Nothing is published yet.

**v4 feature-complete, 2026-08-22: all six In bullets accepted through the page's
own machinery — six bullets, twenty drill rounds, threads 67–91, roughly 48 hours
from blessing to done.** Round 20 (thread 91) accepted the last one (cell marks).
The version is bumped to 0.4.0, the daemon runs it, the suite is green at 71, and
**the release round is live on V4.sv** (round 21, the v2 precedent — the release
line signs itself): the author's approve is the order to tag v0.4.0 (release.yml
builds the binaries) and publish to crates.io. Nothing is tagged or published until
that round comes back approved.

**Cell marks are in, 2026-08-22 — the last In bullet built, drill open.**
`_sv_cell_<col>` beside the shipped `_sv_row`: the directive column marks the named
shown column's cell with add/del/mod, rendered as a deeper wash of the row duotone
(26% ink vs the row's 12%; the daily case is a mod row whose changed cells go
deeper). A directive naming no shown column is a silent no-op, same policy as an
out-of-range freeze. The CSS deliberately matches the row rules' specificity
(`tr > td.…`) so file order decides — the round-6 lesson applied preemptively. The
naming question turned out to be live in a way the drill first got wrong: the agent
checked the sqlnow-mcp repo, declared "nothing to alias", and **the author corrected
it** — the vocabulary lives in *querier's AGENTS.md*: sqlnow reserves the whole
`_sqlnow_` prefix (hidden from grid and exports) with `_sqlnow_format_<col>`
(per-cell styles whose named values include added/changed/removed — the true
counterpart of a sideview mark), `_sqlnow_cell_<col>` (a rich JSON widget: bar,
sparkline, tags — a **false friend** of sideview's `_sv_cell_<col>`),
`_sqlnow_column_<col>` and `_sqlnow_row_height`. Round 19 puts the real fork to the
author: hide `_sqlnow_*` in sv-csv, honor `_sqlnow_format_` marks for
one-file-both-viewers interop, and whether the native spelling should become
`_sv_mark_<col>` to kill the false friend. The lesson for fact-finding: "checked the
repo" must mean the repo where the docs actually live.
**Round 19 answered all as suggested (thread 90) and everything is in:** `_sqlnow_*`
columns hide; `_sqlnow_format_<col>` renders as marks with both vocabularies
accepted in both directives (add/added, del/removed, mod/changed; other sqlnow
style words no-op); the native spelling is **`_sv_mark_<col>`** — the false friend
never shipped. The round's rider asked whether the approach could also produce
git-style inline diffs (both values in the cell) and side-by-side old/new columns
— it can, with zero new machinery, because the marks don't care what the cell text
says: `csvdiff.py --style inline` (changed cells read `old -> new`) and `--style
split` (old/new column pairs, old del + new add where changed), both rendered live
on V4.sv beside the marks style, same sources, same script. 71 tests. Round 20 is
the acceptance.
Round 18 also asked for a real data diff, and the demo is now a worked one:
`examples/csvdiff.py` (key-joined compare, `_sv_row` + per-column `_sv_cell_<col>`
marks, unchanged rows passing through for context) run over
examples/data/readings-2025.csv / readings-2026.csv, its output rendered live on
V4.sv — overwriting the output re-renders the table, the sv-csv file-watch law
demonstrated en route. 70 tests. Round 19 is open; its approval accepts the final
bullet and starts the release sweep toward 0.4.0.

**The grill skill is written, 2026-08-22 — the sixth bullet built, drill open.**
`skills/sideview-grill/SKILL.md`, embedded beside the main skill; `skill install` /
`uninstall` / `status` now treat the two as one set (skill.rs carries a `SKILLS`
table; status gives one verdict per harness across the pair). The skill is the
grilling ritual — design tree, frontier, rounds with recommended answers, facts are
the agent's job — with sv-ask rounds as the medium: `- * ` recommendations
pre-selected, one `role="close"` block per round, `watch` between rounds, a
design-tree prose block kept current at the top of the page, machine-mail
conventions (resolve after folding), and the final confirmation itself a
one-close-block round. Derived with credit from mattpocock/skills (MIT), per the
bullet. Installed live on all four harnesses. **Round 17 approved same hour, all as
suggested (thread 88): the fifth bullet accepted.** Committed. One In bullet
remains — `_sv_cell_<col>` marks — then the acceptance sweep and 0.4.0.

**Editing from the page is in, 2026-08-21 — the fourth bullet built, drill open.**
Both tiers exactly as V4.sv specifies. Migration v5 adds `comments.kind`
('comment' | 'edit' | 'edited'). Tier one, prose: GET `/api/source` hands the editor
raw canon plus a hash; POST `/api/edit` splices under the same sidecar flock the CLI
takes, 409s with current canon when the hash is stale (the agent moved the text —
warn once, then the human wins), and mints a `kind='edited'` machine-mail record so
watch stays honest ('edited' is never postable through /api/comments — forgery
refused, test-pinned). Tier two, everything else: the same editor posts a
`kind='edit'` comment, the proposal fenced in ```md, a visible thread the agent
merges/replies/resolves. Client: the selection chip grew the second verb (comment ·
edit, a strip, never a menu); double-click now produces a selection + chip rather
than straight-to-draft (thread 63's unified model — one extra tap on commenting,
accepted); SSE patches for a block are held while its editor is open; 'edited'
threads join ask rounds as machine-mail the bar never shows. The skill teaches both
kinds. 69 tests. Round 11 came back from the author's *phone* (thread 77): receipts,
request-threads and warn-then-human-wins all as suggested, the unified-chip verdict
deferred until desktop — and edit-on-touch pulled into v4 ("only testing on mobile
now so need it"). Shipped same day: while a selection is active a pencil rides a
second fixed corner button below the comment bubble (the corner-chip law: never
fight the OS toolbar's airspace; visibility is pure CSS off body.sv-selecting), and
the editor textarea is 16px on touch so iOS doesn't zoom-jump (comment-sheet saga,
lessons 7 and 4). En route the author spliced a heading from the phone — the first
real prose edit through the page, receipt on watch, loop verified. Round 12 (thread
79) reported two mobile UX gaps, both fixed same day: a double-tap's selection
collapses almost instantly on mobile and the 700ms remembered-selection grace killed
the corner buttons before a choice could be made (touch now holds 5s), and with
straight-to-draft retired the corner strip is the next step and has to read like it
— both verbs go filled ink while a selection is held. Round 13 (thread 80) then
caught the opposite failure, stuck-forever highlights, and its cause was structural:
two paths set the selecting state and only one armed the clearing timer (a
double-tap that produced no follow-up selection event never expired). The fix is one
rule, `armSelClear()`: a live selection holds the state with no expiry; collapse or
an eventless double-tap arms 5s on touch / 700ms on desktop; a verb tap consumes it.
Every path re-arms through the one function — the same prefer-structure lesson as
the mobile sheet. Round 14 (thread 82) revised the request editor's UX: it no longer
replaces the block but opens *under* it with the block outlined in ink (prose keeps
the replace — there the editor IS the block's text), and a request's quote is now
the selected text rather than `edit <id>` (kind, not the quote, marks it a request;
block-id fallback only when nothing was selected). Both editing tiers carried real
phone traffic en route: a prose splice of a drill heading (receipt on watch) and a
test edit request (merged-nothing, replied, resolved). Round 15 asks the author's
floated question — should requests hide from the bar like machine-mail? (my lean:
no — an unmerged request needs a visible pending state); approval accepts the
bullet. Uncommitted.

Round 15 settled requests-stay-visible (author, all as suggested) and caught the
last UX gap: a tall prose block swaps for a shorter textarea on edit and the
viewport stays put, stranding the editor's top off-screen above. Fixed: the editor
scrolls itself into view on open when its top isn't visible, landing below the
sticky header (scroll-margin, the rail-jump law). **Round 16 approved, 2026-08-22
(thread 87): the editing bullet is the fifth accepted** — six drill rounds (11–16),
a genuine field test from phone and desktop with both tiers carrying real edits
mid-drill. The approval's rider fixed a long-standing bar bug in the same change: a
new comment draft now scrolls the bar to the top, where draft cards are born,
instead of leaving them hidden under a scrolled conversation. Committed. Remaining
In bullets: the `sideview-grill` skill and `_sv_cell` marks — then 0.4.0.

**v4 underway: the ask block is in, 2026-08-21** — the first In bullet, built the day
after blessing. `sv-ask` renders questions (column-0 `- ` lines become single-choice
options; every question carries a free-text rider) and `role="close"` renders the
round's finish control: the three-verdict vocabulary, an optional closing note, a
preview of exactly what the agent will receive, send disabled until a verdict.
Answers draft client-side (localStorage per page+round, freely revisitable); the
finish press posts **one comment per round** through the ordinary `/api/comments` — a
thread on the close block, quote `ask round N`, first line `ask round N — <verdict>`,
then one quoted entry per question in page order — so `sideview watch` covers rounds
unchanged. Sent-state is derived from the thread (never held locally), and an amended
send replies into the round's thread rather than minting a second one. Zero
migration, zero new endpoints: format.rs needed nothing (any `sv-` tag already
parses), render.rs dispatches to the new `ask.rs`, `ask` joined the reserved
extension names, the skill teaches the round convention. Verified live end to end
(render → simulated browser send → watch event). The drill ritual's first run — round
1 on V4.sv, the author drilled on the implementation's own surprises through the
feature itself — came back **revise** within the hour (thread 67) and the revisions
shipped the same day: `pick="many"` multi-select (checkboxes, one quoted line per
pick), `- * ` suggested answers badged **and pre-selected** (the closing note's ask:
an untouched question submits exactly what the reader sees selected, never an
invisible default), and sent rounds collapsing to their answers — complete but never
locked, each block reopening via *change*, an amended send replying into the round's
one thread. Round 2 came back **revise** too (thread 68) and its revisions are in:
the WHOLE round folds to one line on its close block (question blocks leave the flow
until *change*), the ink bar marks current asks only, and the round comment went
terse — questions numbered in page order, only deltas from the suggestions listed,
`everything else as suggested` covering the rest (the diff needs no server help:
`defaultChecked` is the suggestion). Round 3 (thread 69) settled the conversation's
place: round threads are **machine-mail** — hidden from the comment bar entirely
(badge, dots and turn indicator included), keyed by question block id rather than
Q-numbers since only the agent reads them, and resolved *by the agent* once a round
is folded into canon, because the reader no longer has a UI to resolve them from.
The reader's verbs on a folded round became *view* and *hide* (the author disliked
"change"; nothing ever locks, send-again stays when open). Round 4 (thread 70)
confirmed machine-mail and view/hide, and ordered two trims, both in: the preview
button retired (machine-mail with suggestions visible left it no audience) and a
folded round hides its close preamble too — the ✓ line is all that remains. Round 5
(thread 71) cut the close vocabulary to **approved / revise**: with the note field
always present, "approved with notes" was derivable from approved-plus-note, and a
derivable verdict is noise. Round 6 (thread 72) caught a real bug the author saw as
design: the rule clearing the ink bar on folded rounds lost a CSS specificity fight
to `section.sv-ask`'s border and silently never applied — the mobile-sheet saga's
first lesson, relearned; the fix keys the bar off **sent-ness** (`sv-ask-settled`),
so a sent round stays barless even while viewed expanded, and ink marks only rounds
still wanting answers. The author also live-verified send-again (an amended round-6
send landed as a reply on the round's one thread, as designed). **Round 7 approved,
2026-08-21 (thread 73): the ask block is the first In bullet accepted through the
page's own machinery** — seven rounds, run on the feature itself, each revision
argued, shipped and re-drilled the same day. The drill scaffolding is folded off
V4.sv (every decision is recorded in the bullet with its thread); committed as
409647a. The commit also brought V4.sv and VISION.sv into the repo for the first
time — HANDOFF had called VISION.sv committed before it actually was.

**The daily-driver bullet followed the same day: `sideview restart` + louder store
identity, built and proved live 2026-08-21.** Kill-first exactly as the bullet
specifies — SIGTERM the recorded pid, wait up to 10s for the row to clear AND the
port to actually free (never escalating to SIGKILL; the bail is honest), then spawn
detached on the remembered port. The reachability probe runs BEFORE the kill, so
inside a sandbox restart refuses up front instead of trading a working daemon for an
unreachable one. It performed its own first restart (stopped pid 626153, came back on
the same port 46423) and refused correctly from the sandbox with the daemon
untouched. Skew cure texts in `status` and bare `sideview` now name it. Identity:
every write ends with `→ <project root>` on stderr (prose/markup/html/diff/update/rm
joining comment/working); reads stay quiet. **Round 8 approved same day (thread 74),
one rider folded: restart is the user's command, never the agent's — the skill now
says so** (an agent that sees skew relays the command instead of running it; its
writes land in the file either way). Committed as 24a6077.

**The logo bullet followed: the reviewer wears the state, 2026-08-21.** `logo.svg`
inlined into the header beside the wordmark (currentColor, so the theme toggle moves
it) and `static/favicon.svg` in the tab (a favicon can't see the page's theme
override, so it follows the OS scheme in the same ink blues). The state dot is
retired: open and glancing = live; half-lidded, pupils dropped, gentle pulse =
reconnecting; lids drawn and pupils hidden = gone. "Gone" is honestly a duration —
EventSource retries forever and can't tell restarting from dead, so the lids draw
after 8s of failed retries (the timer must not re-arm per error event). Lids are
separate SVG elements toggled by body state classes: structure over override, the
mobile saga's other lesson. Round 9 (thread 75) approved gone-as-duration and the
favicon's OS-theme choice, and revised the dead face: **closed eyes rest symmetric**
— the raised brow is a live expression, so when the lids draw it swaps for a resting
brow, a second element rather than a tweak to the first. **Round 10 approved
(thread 76): the logo bullet is the third accepted.** Committed. Remaining In
bullets: block editing (two tiers), the `sideview-grill` skill, `_sv_cell` marks.

**v4 curated and the vision settled, 2026-08-20, through a grill run on the machinery
itself.** [VISION.sv](VISION.sv) (committed, registered) now holds the north-star — "a
CLI agent's plan deserves what a pull request gets: proper rendering and proper review"
— plus the audience decision (design for n=1, package for the niche, mass appeal dropped),
the positioning survey (comment-loop space crowded and review-session-shaped: Plannotator
7.9k★, crit, difit, Ultraplan, Artifacts; the persistent live document with data by
reference stays uncontested), and the primary list. The author cut v4's In list to six:
the ask block, block editing from the page (two-tier: prose splices directly, other
blocks send md edit requests the agent merges), `sideview restart` + louder store
identity, `_sv_cell` marks, wiring the logo into favicon/header, and a second embedded
skill (`sideview-grill`, the grilling ritual through the page, sequenced after the ask
block, derived with credit from mattpocock/skills, MIT) — [V4.sv](V4.sv). **Goal
blessed 2026-08-20 (thread 66 on V4.sv)** with a working order and a ritual attached:
sv-ask ships first, and each completed In bullet earns a drill — an sv-ask round over
whatever the implementation did that the plan did not predict, on V4.sv or a sibling
page. The git cluster
(watched changesets, `l:` anchors) is parked behind a named trigger: hunk stops sufficing
at n=1. A logo was chosen through seven live iterations on the page (`logo.svg`, "the
reviewer": specs glancing right, one brow soft-raised); wiring it into favicon/header is
in the pool. Two comparison pages exist (crit, plannotator). Found en route: `sideview
update --type` defaults to prose and silently re-types markup blocks (in the pool).

**v4 curation opened, 2026-08-19.** V3.sv moved to the shipped category with its closing
note, and IDEAS.sv was rebuilt as the v4 pool: v3's shipped items removed, its deferrals
(the sqlnow table tier, AUR) and everything still standing carried forward. A V4.sv In
list comes when the author curates, same ritual as v3's.

**The mobile comment-sheet saga (2026-08-09, thread 35 on V3.sv): seven causes, and the
lesson is the same one the v1 diff saga taught.** The author was phone-only for the session, so
every fix was made blind and verified by screenshot, and each of these looked like the previous
fix "not working". (1) The fold control stayed a chevron because the mobile rules were written
*earlier in the stylesheet* than the base rule they override: equal specificity, later wins, so
they never applied on any device. (2) iOS paired fresh JS with stale CSS despite
`Cache-Control: no-cache`, so asset URLs now carry a per-daemon-start `?v=` stamp (a restart is
already how a new binary arrives). (3) The sheet reached `top: 0` but sat *under* the header in
stacking order (z-index 5 vs 10); covering chrome means out-layering it. (4) `overflow: hidden`
on body does not stop touch scrolling on iOS. The real lock is `position: fixed` with the scroll
offset remembered and restored. (5) `position: fixed` elements keep *layout*-viewport size while
the keyboard shrinks the *visual* one, so the sheet is pinned to `visualViewport` height/offset
and the focused textarea is scrolled back into view on resize. (6) A `position: sticky` title
inside the scroller drew mid-thread and left a transparent gap conversation showed through.
Structure replaced it (flex column: title row, then a scrolling sibling), which cannot fail that
way. (7) iOS zooms on focus for any field under **16px**, and the zoom pushed the close cross
off-screen. The two rules worth carrying: **measure on the device instead of theorizing from
screenshots** (a `#svdebug` probe now reports visual viewport, lock state and element rects
live; add it to the URL hash), and **prefer structure over override** when a layout keeps
needing another patch.

**v3 shipped: 0.3.0, 2026-08-16.** Seven days from curation to release, every feature argued
on the page before its code. What 0.3.0 carries over 0.2.1: comment attachments end to end
(migration v4, upload, tokens, markdown bodies, gc) with the mobile comment sheet and
resizable rails; three page formats plus `.sideview.toml` (docs stay markdown and become live
pages); page categories with the one-category strip and `/home`; the extension mechanism
(frame mode, `call_cli`/`call_cli_streaming`, duckdb + git references); Vue islands via the
import map; and `sv-csv`, the table's default tier: server-rendered, diff-tinted, frozen
columns, live on file change. Binary releases: tag-triggered workflow, musl-static Linux
x86_64/aarch64 + macOS both arches, fat LTO (the 90→35 MB probe finding). Deferred out of the
gate by the author at release: the sqlnow extension tier, AUR, and the per-item acceptance
pass (items were resolved thread-by-thread as they shipped).

**0.3.1, 2026-08-18.** The post-release navigation fix, shipped so the binaries carry it.
After the back button showed the SPA holding comments and layout across pages, routing moved
fully server-side: the client router is deleted, every navigation is a real link, and `/`
302s to the most-active page. Also in: the chip styling that survives the label's
button→anchor move. No state crosses pages in this app, so nothing was lost by the deletion.

**The extension mechanism is in (2026-08-15).** EXTENSIONS.md's `frame` mode and two-function
API, built with two working references (`extensions/duckdb`, `extensions/git`) and verified
live end to end: the entry served with `<base>` + `SIDEVIEW_BLOCK` + the API + live theme,
blocks with a registered tag rendering as same-origin frames the parent measures, `call_cli` /
`call_cli_streaming` execing the manifest's binary (fresh process, argv, no shell, 30s/64MB
caps, abort kills the child). The duckdb CLI (v1.5.5, ~/.local/bin) streams `-jsonlines` into
a Vue grid; git colorizes its own repo's log and diffs. 59 tests. The next act is the author's:
an agent in the sqlnow repo builds the real table extension against EXTENSIONS.md alone, as a
test of the docs.

**v3 underway, 2026-08-09: the attachments backend is in** (same day the author curated
V3.sv through the page and blessed its goal). Migration v4 (the `attachments` table:
metadata in db, bytes on disk, V3.sv's model verbatim), the raw-bytes upload endpoint
(sniffed mime, sha-addressed dedupe, 20MB cap), rows born at comment-send with a
confinement check that keeps every stored path inside `.sideview/attachments/` (a row is a
future deletion), watch events and the threads snapshot carrying whole rows, `page rm`
cascading files with a shared-file guard, and `sideview attachments gc [--resolved]` per
the lifecycle law. 50 tests; the loop smoke-tested live end to end. UI half (compose
tokens, card redesign, resizable bar) is next.

**0.2.1, 2026-08-09.** A same-day patch: 0.2.0's new global `--project <dir>` shared a clap
arg id with `skill install --project` (bool), globals propagate into subcommands, and clap
panics on the typed access, so every `skill install` died on arrival. The subcommand flag is
now `--repo` (its help text always said "for committing to the repo"); a `debug_assert` +
parse test pins the rule for every future arg. Found dogfooding the post-release skill
re-sync; V3.sv and IDEAS.sv (the v3 curation pool) were opened the same day.

**v2 shipped: 0.2.0, 2026-08-09.** Merged and published the moment the author ordered the
release *through a comment on the goal's own release line*, completing the done-when exactly
as written: resurrection first (ran live, twice counting the e2e), every section heading
signed off from the browser, each sign-off picked up through watch with the chat silent, and
this publish. Two days from overnight build to release, every revision in between requested
and verified through the page itself.

**Historical (the overnight build note, as the review found it):** v2 built on branch `v2`,
overnight 2026-08-07 (agent, for the author's morning review). Everything V2.sv commits to
except watched diffs (re-sequenced out by the author) is implemented and green (43 tests):
migration v2 (sessions→bindings + the threads/comments/outlines tables), `page set/rm/promote`
with session aliases, `open <file>`, `comment`/`resolve --undo`/`watch --claim` (typed
JSON-lines; the whole loop verified live: CLI comment, watch replay, live resolve event,
exactly-once claims), the browser endpoints plus a per-page `threads` SSE snapshot, the
margin-mark/count-dot/popover/tail-list UI, the iframe envelope (size out, theme in; 85vh
retired), sv-note rendering, explicit outlines verbatim in the rail, and startup rediscovery.
The resurrection test ran live and passed (db deleted, page back from canon, conversation
gone). Binary installed and both project daemons restarted on it; both stores migrated with
`-pre-v2` backups beside them.

Honest gaps for the review: the comment UI has had no human visual pass (built to the CSS
system, never seen by eyes); paragraph anchors hash in JS only (`anchorHash`, FNV-1a 64/48,
vector pinned in app.js; the rust twin belongs with diff re-resolution, which didn't start);
`l:` anchors and per-line diff comments are unimplemented; sv-note renders in place with a
reference line rather than physically at its anchor; explicit outlines assume scrollspy
(tabs+spec degrades to all-visible); V2.sv's sign-off ritual (comment every heading from the
browser, agent picks each up via watch) awaits the author.

## Older state

**v1 shipped: 0.1.0 published to crates.io, 2026-08-07.** Every done-when bar in V1.md was
met: pages are files (verified by deleting the only db, twice), broken files heal on save,
deletion is file removal from page and CLI, code highlights in both themes, multi-file diffs
render unified and side-by-side with rail navigation, and the four-harness matrix ran live,
all on OpenAI models, with the author manually confirming all three foreign harnesses before
release. v2's core is already designed with reasons attached (V1.md's committed-to-v2
entries): watched diffs via gitoxide, comments in the db behind `sideview watch`, Sphinx-style
hover placement, explicit agent outlines, and the dividing principle that governs them all:
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
  port. The old daemon still holds it at bind time, so the second falls back to an ephemeral
  one. "The recorded port is reused" therefore holds across restart-after-exit (the case that
  matters for SSE reconnection) but not across supersession. Acceptable; noting it so the
  claim in V0.md's port section is read with that asterisk.

**The v0 → v1 line, drawn by the author on 2026-08-04.** v0 closed with the file-endpoint
exclusion and `--detach` printing the tailnet URLs. Moved to v1: iframe autosizing (fixed 24rem
until ResizeObserver + postMessage), staging the "Done when" regression trap (stale namespaced
row, claimed by bare `sideview`; implemented, never staged), the third done-when bar (skill
offers the sandbox-disabled `--detach` with nothing running), and the `tailscale serve`
SSE-buffering check. **v1 itself is the real dogfood**: genuine work sessions writing genuine
plans through the skill. Every experiment so far was showcase-shaped, and the product is plans.
Deprioritized rather than moved: unknown-class/`style=` logging. Full Bootstrap made silent
no-ops mostly moot; the `TODO` stays in render.rs for when the vocabulary-data curiosity
returns. Still open by design: the spawn-lock release window (healed by supersession) and the
provisional scroll behaviour. (Historical: session labels gained a writer, and the two
code-review bugs, swallowed SSE `Lagged` and unencoded session ids, were fixed with pinning
tests before the first commit.)

**v1 was scoped the same evening (2026-08-04), by the author, then re-founded within hours:
[V1.md](V1.md).** The goal is 0.1.0 on crates.io. Dogfooding the plan immediately exposed the
canonicality dichotomy (the plan existed as V1.md *and* as page blocks; which is correct?),
and the discussion landed somewhere bigger than the original scope: **pages are files.** Every
page's canonical source becomes a `.sv` block document, throwaway ones implicit in
`.sideview/pages/`, document ones committed in the repo, with the db demoted to daemon
bookkeeping, bindings and derived replay state ("delete the db and no content is lost"). The
rest of v1 lands on top: session deletion (= deleting the file; three earlier designs and the
whole rev-counter problem dissolved, see V1.md), harness independence proven live in
codex/opencode/pi, code highlighting (syntect, class-based, duotone), and diff blocks
(`git diff | sideview diff`, file paths as outline headings). Explicitly deferred: tables and
app subprocesses. Also set at scoping, author's rule: **dogfood first**. Nothing is
implemented before its design has appeared on the live page.

**2026-08-05: the foundation landed. Pages are files, live.** The format survived a full day
of adversarial probing (V1.md's stress-test section) before a line was written; the author's
sign-off included deleting the only store, so there was no migration and the v0 schema is
simply gone. What shipped: `format.rs` (the fence scanner, with implicit-close healing and the
column-0 rule, which the plan page itself needed on day one for its own format examples),
authoring as locked atomic file splicing, the daemon rebuilt around in-memory state derived
from files (stat-polling bindings, reparse-diff by id, full-state SSE connections, which
dissolved `Last-Event-ID`, tombstones and the rev counter in one move), and `spec.rs` deleted.
Verified live: CLI append/rm round-trip through the file; a raw `sed` on the page file patched
exactly one block over SSE; the db was deleted and rebuilt with the page content intact, so
the delete-the-db test passed for real. The skill gained its your-page-is-a-file paragraph
(direct edits are equivalent to the CLI; never escape inside a block; tags count at column 0)
and was re-installed current.

**Code highlighting landed 2026-08-05**: syntect through comrak's adapter (`syntect-fancy`,
pure Rust), class-based with an `sv-` prefix, in the same render pass as the markdown. The
duotone treatment lives in sideview.css: keywords/storage in ink, operators deliberately
exempted (inking every `=` is noise), entities by weight not color, strings/comments in grays,
one rule set for both themes since every color is a token that swaps. Mermaid fences keep
their `code.language-mermaid` contract (the client reads `textContent`, so syntect's spans are
harmless), test-pinned. Cost: +1.1 MB on the binary (16.2 → 17.3 MB), the embedded default
syntax set; fine against crates.io's 10 MB *package* cap since the syntax set ships inside
syntect, not our crate.

**Diff blocks landed 2026-08-05**: `git diff | sideview diff`, the fourth block type. diffy's
`PatchSet` parses (git extended format: renames, creates, binary entries, all titled
honestly); the aligned model is ours (removed/added runs paired index-wise, unpaired lines
against empty cells); `similar` marks word-level `<del>`/`<ins>` on paired lines, gated by
*character*-level ratio ≥ 0.4, because word tokenization counts whitespace as matches and
flatters unrelated lines, a bug the tests caught on day one. Both views render into one HTML
string (inactive hidden by `data-view`); `view=` on the block is the agent's default, the
client toggle is the viewer's override remembered per block, narrow screens collapse to
unified. File paths are outline h2s with anchors, so the rail navigates a multi-file diff
(verified live on the session-deletion commit itself: 8 files, 8 rail sections). Garbage
degrades to raw mono with an honest note; a mid-diff parse failure renders the files that
parsed plus a visible "rest could not be parsed". Duotone tints: additions lean ink, removals
lean the warm tone, intraline is a deeper wash of the same. Deferred within diff (V1.md):
syntax highlighting inside lines, `src=` references; watched diffs are committed to v2 via
gitoxide.

**The mobile diff saga (2026-08-05, evening): six distinct causes, worth remembering.** The
author's phone (Chrome on iOS) showed the diff oversized with wrapping numbers, and the fix
took six real findings, pinned by a live probe block reporting computed styles from the
device. (1) Number cells inherited `pre-wrap`/`break-word` and wrapped digits. (2) Below 992px
the rail bows out but `body.sv-rail #sv-blocks` kept 6rem of desktop side padding, a pure
phantom gutter. (3) Embedded assets change on upgrade behind unchanged URLs, so phones showed
stale CSS; assets now send `Cache-Control: no-cache`. (4) **WebKit text autosizing inflates a
*container's* computed font-size and lets inheritance carry the boost into children, even with
`text-size-adjust: none` set and reported, but an element's own rem declaration computes
against the root and escapes.** Any deliberately small type must be declared on the element
holding the text, not inherited (this is why the diff font "never changed" through three
attempts). (5) Empty cells have no line box, so blank diff lines rendered squashed; the fix is
`td:empty::before { content: "\00a0" }`. (6) The prose layer's `.sv-block td` (equal
specificity, later in the file) silently beat every `.sv-diff-table td` padding on *all*
platforms; diff tables are now excluded from prose-table treatment via `:not()`. Landed
sizes: 0.8rem desktop, 0.78rem mobile with one-line-per-row horizontal scroll inside the
figure, 1px vertical cell padding so consecutive intraline washes don't fuse.

**The harness matrix began 2026-08-05 (evening).** codex 0.146.1, opencode 1.18.13 and pi
0.73.1 are installed via npm (auth pending, since OAuth flows need the author at the machine);
`sideview skill install` reaches all four harnesses and `status` reports per-harness drift.
First hard finding, measured under `codex sandbox` before any auth existed: **codex's
landlock+seccomp denies `socket()` outright**. There is no network namespace, so every one of
netcheck's namespace tells reads clean, the old verdict said "reachable", and a spawned
daemon would have died at bind with the error visible only in daemon.log. The verdict now
leads with a decisive universal probe (bind a loopback listener; on error, refuse with the
reason), so sideview under codex-default now prints the honest one-line instruction, same as
under Claude. Still open for the authed runs: whether codex with `network_access=true`
lets auto-spawn *work* (no namespace means a permitted socket is genuinely reachable),
opencode and pi end-to-end (neither sandboxes by default, so auto-spawn should just work),
and their session-identity env vars. `codex sandbox`'s default is read-only fs, so the
store-write path also waits for the real `codex exec` run.

**The harness matrix ran live 2026-08-06: all three legs pass, all on OpenAI models** (the
author's provider choice, which made it a cross-model-family test of the skill and format).
Per harness, in a shared scratch project, each run's env captured to a file for ground truth:

- **codex** (exec, `--full-auto`; its default exec sandbox is read-only and even blocks
  `env > file`): skill activated, two blocks + label written, and the netcheck socket-probe
  fix fired exactly as designed: the honest "no daemon running — run `sideview` in …" line
  under a sandbox that denies `socket()`. No session id in its shell env (`CODEX_THREAD_ID`
  is MCP-only in 0.146); falls to the cwd rung.
- **opencode** (run): skill activated, and **auto-spawn worked, the first agent-started
  daemon in the project's history** (no namespace, permitted sockets; pid claimed the row,
  page served 200, the browser tab opened on the author's desktop, which unsandboxed is the
  right UX). Exposes `OPENCODE_PID`, now a session rung (`opencode:<pid>`).
- **pi** (`-p`): skill activated, block written against the already-running daemon (third
  daemon path, silent success). Markers only (`PI_CODING_AGENT`), no id, so it falls to the
  cwd rung.

Findings that became code: the `OPENCODE_PID` rung; `--session ''` (a codex model invented
the spelling) minted an empty-id session, so empty explicit ids now fall through. Findings
recorded, not coded: env-less harnesses sharing a project share the cwd session (pi's label
overwrote codex's mid-matrix, coarse identity working as designed); one contaminated first
run (launching codex from inside Claude Code leaks `CLAUDE_CODE_SESSION_ID`, so matrix runs
must scrub the env). Claude Code's own leg is this entire project's history.

**Session deletion followed the same day.** Deleting a page is deleting its file: `sideview
session rm [id]` (no id = your own; never auto-spawns a daemon) and `DELETE
/api/sessions/{id}`, the page's first write, behind the ✕ on the session chip, two-step and
self-disarming, tidying power rather than authoring power. Both remove file + sidecar lock +
binding; the poll loop notices the binding vanish, and every tab converges because the client
now treats the sessions snapshot as authoritative (blocks of unlisted sessions are dropped,
which is also what heals a tab that slept through a deletion). Verified live in both
directions; the scratch sessions used for the test were themselves deleted through the
feature. 28 tests.

**Later the same day (2026-08-04), the design system switched to real Bootstrap 5**: vendored
v5.3.8, CSS only, with a prose layer for bare markdown elements and a v4-compat shim in
`sideview.css`. Pico is gone. V0.md's design-system section records the reversal, why the
borrowed-subset approach lost, and the Tailwind/daisyUI rejection.

**2026-08-04: blocks declare their headings, and the page grew an outline sidebar.** The SSE
event carries `headings` (see V0.md's frontend section for derivation rules per type). The
outline is optional from both sides: a viewer toggle in the header, and agent-side
`sideview session set --label … --outline auto|off`, which brought session properties in as one
chunk and finally gave `label` a writer. Properties then became a single JSON `props` field
(reasons in V0.md, chiefly that every `user_version` bump hard-stops older binaries, too high a
price per cosmetic flag). The interim migration steps that got here were **squashed back to a
single v1 the same day**, per V0.md's pre-release rule, and the one existing store was deleted
by hand, but not before the machinery had been exercised for real on live data: `user_version`
stepping, the pre-migration backup on a non-additive step, and a column-fold via `json_patch`
all ran and worked. Two knock-on facts worth knowing: `scraper` is now a dependency, which
unblocks the unknown-class/`style=` logging TODO in `render.rs` (the lenient HTML parser it was
waiting for is in the tree); and dogfooding this found two more bugs, both fixed: the
remembered port lived only on the daemon row so a *clean* restart forgot it (now durable in the
`meta` table), and CLI output piped into `head` panicked on EPIPE (SIGPIPE now restored to
default). Same session, earlier: `shutdown_timeout(1)`, because a page holding its SSE stream
open otherwise made every daemon shutdown take the full 30s grace period.

Otherwise: six documents, an MIT licence, no remote.

The 2026-08-03 review changed V0.md in ways worth knowing about if you read it before then:

- **Sandbox detection is in, and `$CLAUDECODE` is not the test**: it is set both inside and
  outside the sandbox. The rule is now *auto-spawn unless the namespace is provably unreachable*
  (no non-loopback interface, no routes), with the daemon recording `netns` and `reachable` on
  its row. This closes a failure the design walked into: a daemon spawned inside a sandbox
  answers ping/pong perfectly and looks healthy to everything except the browser.
- **The CLI cannot escalate, but the agent can**, via the harness permission gate: re-running
  with the sandbox disabled is verified to produce a surviving host-bound daemon. The skill
  offers it; the printed instruction is the fallback. (A first pass of this review said
  escalation was impossible, conflating the CLI process with the agent driving it.)
- **Remote binding is tailnet-by-default** (`--bind auto`), because detection is free.
  Interface enumeration measures 29µs in-process, once at daemon start, never on a CLI call.
  Detect by the `100.64.0.0/10` CGNAT range rather than the `tailscale0` name, print the raw
  IP rather than a hostname (this host's `hostname` is `cachyos-x8664`, which is *not*
  necessarily its MagicDNS name), never bind the wildcard, and fall back to loopback with a
  printed note when `bind()` gives `EADDRNOTAVAIL`. `--bind loopback` opts out; no token, with
  the revisit trigger being a tailnet node you don't control.
- **Worktrees resolve to the main checkout's store** via `--git-common-dir`, or the one-time
  daemon question becomes per-worktree.
- **`show` and the `image` block are cut entirely** (decided 2026-08-03, after the rest of this
  review). A front door with one type behind it isn't a front door, and `<img src="…">` in a
  markup block reaches the same result with nothing new to learn. The file endpoint and its
  root confinement stay, because `<img>` needs them. Three block types now, not four.
- **`markup` renders with no shadow root**, against DESIGN.md's rung-2 note.
- **A v0 schema exists** in V0.md, along with per-session `short_id`s and a defined fallback
  rendering for unknown block types.
- **`sideview daemon --restart` is gone**, incoherent with the daemon living in your foreground.
- **The Tailscale/token section is gone**: v0 binds loopback; `ssh -L` and `tailscale serve`
  need no code.

The design is settled enough to start coding from [V0.md](V0.md). It went through three
reframes getting there: from "richer plans than markdown", to "embed live pieces of a project",
to "a visual side channel for CLI agents whose content bypasses the model's context". The third
is the one the docs are written around, and it is the one that explains why the design looks
the way it does. Plans are now the flagship *use case*, not the definition.

## Do these before writing code

**Check whether `tailscale serve` buffers SSE.** Ten minutes, and it gates the entire remote
story: the product is one long-lived stream, and if Tailscale's reverse proxy buffers it despite
`X-Accel-Buffering: no`, then direct tailnet binding and `ssh -L` are the only remote paths and
the identity story goes with it. Stand up any trickling SSE endpoint behind `tailscale serve
--bg` and watch whether events arrive one at a time.

**Terminal graphics are no longer on this list.** An earlier version said to set up kitty's
graphics protocol first, on the grounds that if `kitten icat` already solved "show me a
screenshot" then the image block was solving a solved problem. With `show` and the `image` block
cut, the item has lost its reason, and it would have failed anyway: measured in an agent Bash
call, `TERM=xterm-256color`, no `KITTY_WINDOW_ID`, and stdout is a pipe with no tty, so the
escape sequence never reaches the terminal emulator. Terminal graphics need a live tty, which is
the coupling sideview exists to avoid; it works when *you* type the command, not when an agent
runs one.

**The service-block spike, on the other hand, can wait.** The 2026-08-03 review argued for
deferring it; this section originally said the opposite. The spike itself is unchanged and
still worth doing: throwaway code, can a dev server be started, proxied, iframed into a page,
and killed cleanly? It is the long-term thesis and the only experiment that could reshape the
roadmap.

But nothing in v0 depends on the answer, and the thing that will teach you most right now is the
latency feel and the class vocabulary against real plans, both of which need v0 running. So:
**first afternoon v0 is blocked on something else, do the spike.** The original argument for
doing it first was that discovering it in month three is expensive, which is true, and the
counter is that week two is early enough for a capability with no v0 dependents.

Optionally, an hour with [Wave Terminal](https://github.com/wavetermdev/waveterm), whose `wsh`
drives graphical blocks from the shell and is the closest existing thing to this idea. It was
rejected because the display is bound to its client app, but the ergonomics are worth feeling.

## Not documented anywhere else

**The host proxy DOES forward localhost, tested properly 2026-08-03 with a listener actually
running on the host.** An earlier version of this section concluded the opposite; it was wrong,
and the way it was wrong is instructive enough to record. What holds:

- The advertised `CLAUDE_CODE_HOST_HTTP_PROXY_PORT` (`39669`, `36113`; it varies) is **refused**
  from inside. The reachable endpoints are in-namespace `socat` forwarders on `127.0.0.1:3128`
  (HTTP) and `:1080` (SOCKS), which relay to a host-side proxy over a bind-mounted unix socket.
- They need **proxy auth**, and the credentials are in `$HTTP_PROXY`, regenerated per sandbox
  invocation, so nothing can be hardcoded.
- With `python3 -m http.server 8765 --bind 127.0.0.1` running on the host: **200 through both
  the HTTP and SOCKS proxies**, confirmed in the server's own access log. A raw TCP connect to
  the same address is refused, and so is anything that bypasses the proxy.

**Why the first attempt said "hang, therefore blocked":** nothing was listening on the ports
probed (`:9`, `:22`), and `$no_proxy` lists `127.0.0.1`, so curl silently ignored the `-x` proxy
it was supposedly testing and went direct. Both mistakes point the same way: if you re-test
this, start a listener first and clear `no_proxy` explicitly.

**What it changes, and mostly doesn't.** V0's core constraint is untouched: a daemon bound
*inside* the sandbox is invisible to the host in both directions (verified), so the browser
still cannot reach an agent-spawned daemon. What is now false is "the store is the only
channel": a sandboxed CLI can HTTP a host daemon. V0 keeps the store as the mandatory path
anyway (works everywhere, no credentials, ~100ms nobody perceives) and treats the proxy as an
optional better liveness check.

**Escalation is real, via the agent rather than the CLI.** A `setsid nohup`'d listener started
from a sandbox-disabled Bash call binds a host port and **survives after the call returns**,
verified. So the skill can have the agent offer to start the daemon, which is one approval
instead of a thing you type. The CLI process itself still cannot escalate and never prompts.

**One stray process found while testing.** An orphaned `bwrap` from an earlier session is still
running `python3 -m http.server 41777` with a `sideview-bind-test-ok` index, rooted at
`/home/david/compuse`, a leftover from a previous bind experiment. Harmless, but it is the exact
failure mode the design's "teardown validates the lifecycle decision" note is about, arriving
before any code was written.

**The sandbox measurements, for reference**, taken the same day from one sandboxed Bash call and
one with the sandbox disabled. The full table is in V0.md; the short version is that the sandbox
has only `lo` and no routes, `/proc/1/comm` is `bwrap` at pid 2, `uid_map` is `1000 0 1`, and
the net-ns inode is `4026532958` against the host's `4026531833`. `$CLAUDE_CODE_SESSION_ID` is
present and stable (Claude Code 2.1.220), and `$CLAUDE_CODE_CHILD_SESSION=1` accompanies the
*same* session id in subagents.

**Naming research, so it isn't repeated.** `sideview` is free on crates.io. Also checked:
`showme` is taken by a terminal image viewer (adjacently confusing), `vitrine` by a static site
generator, `glance` by a computer-vision crate. `agentview` and `viewfinder` are free on
crates.io, but `agentview` is badly crowded: `agentview/agentview` is a session viewer for
conversational agents and `kenn-io/agentsview` does session analytics for coding agents, both
of which are the "transcript mirror" category sideview is explicitly *not*. The name was chosen
to encode the thesis: a view beside your terminal, fed by a side channel.

**The crate name is unclaimed and the repo has no remote.** Publishing is deliberately left to a
human decision. `gh repo create` when ready.

**LICENSE says "David Raznick" personally**, not Global Energy Monitor. Change it if that's
wrong; it was a judgement call based on this being a personal project directory.

**`~/.claude/skills/hunk-review` is currently a broken symlink** (into a `hunkdiff` npm package
that has moved). Noticed while researching how to ship sideview's own skill, which copies that
distribution model. Worth fixing independently.

## Open, and deliberately so

**Scroll behaviour when a block arrives.** Likely answer: scroll only when already at the
bottom. Left unspecified because it wants a real page in front of you.

**Sessions cannot be deleted, and the design labs made that visible.** The theme/font/combo lab
sessions (2026-08-04) did their job and now sit in the switcher forever: `rm` tombstones blocks
but nothing removes a session, and hard-DELETEing rows by hand would regress `MAX(rev)` and
corrupt `Last-Event-ID` replay. The rev counter must survive any future deletion feature (a
`meta`-held floor, or tombstoned sessions). A `session rm`/archive belongs in the next batch of
session work; until then, labs cost a permanent chip each.

**The file endpoint no longer serves the store's internals.** Noticed 2026-08-04 when `/f/`
got its first real use (`/f/.sideview/sideview.db` was fetchable by any tailnet node), fixed
the same day: `sideview.db*` (backups included), `daemon.log` and `spawn.lock` return 403 by
name, while other files under `.sideview/` still serve; the dogfood comparison pages iframe
their rival entries from there. Pinned by test.

**First controlled dogfood (2026-08-04):** three identical subagents summarized this project
visually: one on sideview (given nothing but the installed skill), one as a local HTML file,
one as a published Claude Artifact. Sideview: 3:04 total, **first content on screen at 71s**,
then a block every ~15–20s; 73k tokens. Artifact: 4:20, nothing visible until done; 80k tokens.
Local HTML: 6:48, nothing until done; 96k tokens (it hand-rolled an entire design system,
exactly the cost V0.md's premise predicts). n=1, agents varied in self-QA thoroughness, so the
totals are indicative; the *shape* (streaming vs single reveal, styled-for-free vs
invent-your-own-CSS) is structural. Skill-tuning observation: the sideview agent wrote 6 of 7
blocks as `markup` rather than `prose`; if prose-first is wanted, the skill has to say so.

**Rounds 2 and 3 (same day) turned the experiment into a tuning loop, and the loop has a cost
curve.** Each round's visual gaps were named, fixed (Plex + print duotone, deeper paper, bigger
display, SVG-first diagram guidance, themed mermaid), and re-tested. Result: sideview's looks
converged toward the leaders while its headline metric inverted. Time-to-first-content went
71s → 106s → 261s as the skill demanded more craft, and in round 3 the artifact beat sideview
on *total* time (5:08 vs 5:53) for the first time. The quality guidance taught agents to
compose before emitting, which is exactly what streaming exists to avoid. Resolved after the
round-3 verdict (sideview's SVG diagram judged best of the three; overall still behind local
HTML's flourishes, near-parity with the artifact): the skill now matches effort to the page's
job. Working plans stream prose-first with diagrams explicitly optional (a mermaid sketch when
a picture genuinely helps); hand-authored SVG is reserved for pages whose point is visual
presentation; and an explicit stream-it instruction says emit early, sharpen with `update`.
Comparison pages: sessions `round2`, `round3` (tabs mode, rivals iframed via `/s/` and `/f/`).

**The round-1 author's verdict on looks inverted the speed ranking: local HTML best, artifact
middle, sideview worst.** Three causes traced, each actionable. (1) The HTML agent invoked the
frontend-design skill (transcript-verified; the others didn't) and had free rein, where
sideview's skill deliberately enforces the house style: speed and consistency bought at a
polish ceiling. (2) Sideview's weakest elements were its hand-drawn diagrams, which makes
**mermaid the deferral with the strongest evidence against it**. V0.md's Out list says "real
demand, but not this version"; the demand is now measured, not predicted. (3) The agent
presented `sideview show readings.parquet` as the headline property because **README.md still
said so**; the 2026-08-03 cut reached V0.md and HANDOFF but never the front door. Fixed the
same day. The doc-rot lesson generalises: a cut isn't done until the docs that *sell* the
feature are updated, not just the ones that specify it. app.js is ~330 lines of hand-rolled
DOM state sync and growing; the itch for a no-build framework (Alpine, petite-vue, Vue's ESM
build) is legitimate. Deliberation so far: two different JS domains are conflating. The *page
chrome* (rail, strip, spy, dot) is our code and small, so vanilla holds until the feedback
channel lands, whose forms and params are the first genuinely framework-shaped work. That is
the natural adoption point, and Alpine or Vue-ESM (both no-build, vendorable as one file, deep
LLM priors) are the candidates; petite-vue is unmaintained, and htmx overlaps what SSE already
does here. **Update (2026-08-08): the trigger fired.** The feedback channel landed and the
popover/tail code is exactly the predicted framework-shaped work; a hard day of scroll
debugging also showed the block-reconciliation bugs belong to a different fix (idiomorph-style
morphing, on V2.sv's candidates). **Adopted 2026-08-08**: the comment-bar redesign (V2.sv's
placement bullet, third design) was the trigger's work arriving, and it shipped as the first
Vue island: vue.esm-browser 3.5.41 vendored, composition API with explicit setup(), inline
template, blocks and chrome staying vanilla. The same change put Access-Control-Allow-Origin:
* on /assets, so html blocks can ESM-import the vendored Vue: the islands prize extends to
agent-authored blocks. Vapor mode was assessed as no threat to the no-build path (opt-in,
build-time, same authoring model, and a vendored file can't rot). Composition API works fully
in the ESM build; only the `<script setup>` sugar needs a compiler (write `setup()` + explicit
return). One real caveat: the runtime template compiler uses `new Function`, so a strict CSP
without unsafe-eval would block it. Remember this if sharing ever grows CSP headers. **The
author's reframe (2026-08-08): the main prize is html blocks as Vue islands**: a vendored vue
at /assets/vendor/ is importable by any srcdoc block, giving artifact-grade interactive blocks
with no CDN, no in-browser JSX transpile, files-in-repo persistence, and the envelope already
sizing/theming them. This benefit arrives from vendoring alone, before any migration of our
own UI. Mechanics: opaque-origin iframes need Access-Control-Allow-Origin: * on /assets for
ESM imports (one safe line), or blocks use the global build via script src. Adoption itself
remains the author's call; vanilla currently holds. **Asked directly on 2026-08-10 (should the
chip strip or the contents rail move to Vue?) and answered "neither now, the rail
eventually"**, on the shape-not-size principle this section already argues: the strip is 43
lines that *already* rebuild wholesale on every snapshot, so a framework replaces nothing (its
one latent bug, a chip's armed delete state living in a closure so a snapshot arriving mid-arm
silently disarms it, is a three-line vanilla fix); the rail is ~113 lines and does keep a
hand-rolled `railRefs` reconciliation map, but half of it is scrollspy, which measures element
positions and stays imperative under any framework. **The trigger to migrate the rail is the
mobile rail feature**, which adds a drawer state machine, the point where its state stops
being derived from scroll and starts being state we manage. Migrating sooner would be a
rewrite with regression risk in the spy and no user-visible gain. The *plugin architecture*
(blocks getting scoped access to parts of the page) should not be answered with a framework at
all: the web-native boundary is custom elements plus a small explicit `window.sideview` API,
which keeps plugins framework-agnostic, gives them shadow-DOM isolation (DESIGN.md's rung-2
note returns here), and lets an agent emit `<sv-something>` as ordinary markup. Constraint to
hold either way: whatever is adopted must vendor as a single static file into rust-embed. No
toolchain, per V0.md's frontend section. **Refined by the author (2026-08-08):
no-external-origins is a *core* principle, not a universal one.** Extensions, the
custom-element layer above, may load established libraries from CDN (SRI-pinned): an extension
is already a trust decision, so its external dependencies just make that visible, and offline
it degrades visibly while core never degrades at all. First resident: mermaid, **removed from
core 2026-08-08**. Its weight (3.6MB, 70% of vendor/, parsed on every page load) and stock
aesthetics (round-2 verdict: dated; agents draw better SVG by hand) argued it out. Fences
degrade to highlighted code (the language- class survives for the future extension to key on),
the theme toggle became CSS-only in the same stroke (the full-page rebuild existed solely
because mermaid SVGs bake their colors), and the .crate dropped by roughly a megabyte.

**React-controlled blocks, a ladder not a decision** (2026-08-04, prompted by wanting a
Glide-grid table like querier's). React never controls the page; per-block roots or iframes
only. Rungs, cheap to expensive. (0) *Already works*: a Vite `dist/` copied into the project
and iframed via the file endpoint (`/f/…`, build with `base: './'`); this is also the
service-block spike arriving through the front door, since an iframe can equally point at a
running app's own port. (1) The deferred `table` block ships as a sideview-precompiled custom
element wrapping Glide, vendored like Bootstrap: node becomes a maintainer-time toolchain, the
one-binary user promise survives, and `{sql}` finally exercises reference-never-embed. (2)
Pane takeover is just a session property in the props bag (no migration): one block filling
the viewport below the header. (3) Artifacts parity, agents writing TSX against a pinned
import map of vendored ESM, would use SWC embedded in the daemon (Rust, transpile at write
time, browser runs native modules), not a browser-side Babel. The Vue analog of this rung is
**fervid** (all-Rust SFC compiler, drop-in ambition for @vue/compiler-sfc; alpha as of
2026-08) — daemon compiles .vue at serve time like it renders markdown, which would also
allow the smaller runtime-only Vue build. Asked and answered as curiosity on V5 thread 119;
browser-side compiler-sfc (~1MB+) and Vite (the Node toolchain) were ruled out there, and
the author judged fervid itself too early and a risk — the settled approach for island
readability is decomposition into child components with annotated template literals. Take rung 0 as an early
experiment; take rung 3 only if 0–2 prove insufficient.

**The `sv-` class list.** Six to ten classes for what Bootstrap doesn't cover: metric/delta,
option cards, decision matrix. Needs designing against real plans, not in the abstract. (The
companion question of *which* Bootstrap names to implement dissolved on 2026-08-03 when the
design switched from a borrowed subset to vendoring real Bootstrap 5; see V0.md. What remains
derivable from real plans is the `sv-` layer and any v4-shim additions the unknown-class logs
reveal.)

**Which DESIGN.md sections are stale.** Each now carries a marker in place, so this list is
only a map: the schema sketch (predates the cut), "Identifying the session" (tty-based chain),
"Lifecycle" (per machine, idle exit), rung 2's shadow root, and build order. Reconcile them
properly when v0 ships rather than now.

## The conversation's own summary

If a future session wants to know *why* rather than *what*, the reasoning is in the docs rather
than in any transcript; every significant decision was written down with the alternative it
beat. That was deliberate. The most load-bearing pieces of reasoning, in rough order:

1. Content must not pass through the model's context. Everything else follows.
2. The sandbox gives each Bash invocation its own network namespace, so the agent cannot start a
   reachable daemon.
3. A design vocabulary only saves tokens if the model already knows it.
4. If a page has no live blocks, markdown was already the right answer.

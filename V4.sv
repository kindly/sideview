<sv-page label="v4: the page asks back" category="plan" order="3">

<sv-prose id="intro">
# v4: the page asks back

Curated 2026-08-20, straight out of the vision grill that produced [VISION.sv](VISION.sv):
the author cut the primary list to exactly this In list and said "v4 should be started
with that list" (grill page, thread 61). Rationale lives in the design docs and the
grill page; this page holds only what v4 commits to.

Carried principles, not up for re-decision: sv files are canon and the db holds what
should not be versioned · the page file has one author and everything multi-writer goes
through SQLite · reference, never embed · dogfood first.
</sv-prose>

<sv-prose id="in">
## In

- **The ask block (`sv-ask`).** ✓ **Accepted by the author through this page's own
  machinery, 2026-08-21, thread 73 — seven drill rounds, run on the feature itself.**
  Agent-led questions answered *at the block*, not hunted
  for in the bar. Shape settled by grill, 2026-08-19: one tag, `role="close"` for the
  "finish grilling" button; enumerated options plus one free-text rider per question;
  drafts client-side and freely revisitable, completion per-round never per-question;
  the finish press submits **one comment per round**, so
  today's `watch` covers it unchanged; asks share a `round="N"` attribute; the agent
  folds the round into canon. Core, zero migration. Two details adopted from the
  plannotator comparison: a close vocabulary that distinguishes approved / approved with
  notes / revise *(cut to approved / revise in round 5, thread 71: with the note field
  always present, the middle verdict was derivable — noise)*, and a preview of what the
  agent will receive before the round submits *(retired in round 4, thread 70:
  machine-mail comments with suggestions visible on the page left it no audience)*.
  Drill additions (author, 2026-08-21, threads 67–72): `pick="many"` multi-select,
  `- * ` suggested answers badged and pre-selected, whole-round collapse on send —
  *view*/*hide* to unfold and refold, never locked, amended sends replying into the
  round's one thread, the ink bar marking only rounds still wanting answers — and round
  comments as machine-mail: hidden from the bar, keyed by question block id, deltas from
  the suggestions only, resolved by the agent once folded into canon.
- **Editing blocks from the page, two tiers** (author, 2026-08-20, thread 62 on this
  page). ✓ **Accepted by the author, 2026-08-22, thread 87 — drill rounds 11–16, a
  five-round field test from phone and desktop, both tiers carrying real edits
  mid-drill.** Drill decisions: migration v5 (`comments.kind`: comment / edit /
  edited); splice receipts and ask rounds are machine-mail, edit requests stay
  visible in the bar (round 15) with the selected text as their quote (round 14);
  the request editor opens *under* the outlined block while prose replaces (round
  14); conflicts warn once then the human wins (round 11); edit-on-touch pulled into
  v4 (round 12: a corner pencil below the bubble, 16px textarea against iOS zoom);
  one selection-lifetime rule after the stuck-highlight saga (rounds 12–14: live
  selection holds, collapse arms 5s on touch, a verb tap consumes); the editor
  scrolls itself into view (round 16); and 'edited' is unforgeable through the
  comment door. **Entry is the selection chip, and the gesture model unifies** (author,
  2026-08-20, thread 63): double-click or select produces a *selection*, and the chip
  that already knows how to position itself grows two verbs, comment and edit — a
  two-button strip, never a menu. Edit opens the whole block's markdown with the cursor
  at the bit that was clicked. The one extra tap on commenting is accepted; the retreat,
  recorded in thread 63, is edit-inside-the-compose-card, reachable later without
  undoing anything. *Prose: the direct splice.* Raw markdown in a plain textarea, saved
  as a whole-block splice under the existing file lock; a from-hash guard 409s instead
  of clobbering the agent's newer text; an `edit` event kind keeps watch honest. *Every other block type: the edit request.* The same
  textarea affordance, the change written in markdown, landing not as a splice but as
  an edit-marked comment (a flag or event kind on the comment machinery) that the agent
  picks up through watch and merges into the code; the working/replied ladder covers
  the async gap. Direct html editing rejected: one unclosed tag breaks a block, and
  html blocks are often generated code. The split is the one-author rule applied: the
  user writes prose and asks for code. The page's first *authoring* power, the trust
  expansion V1.md said to make on purpose; both tiers stripped server-side under any
  future read-only share.
- **Daily-driver ergonomics.** ✓ **Accepted by the author, 2026-08-21, thread 74
  (drill round 8).** `sideview restart`: kill-first (SIGTERM the recorded pid,
  wait for the port to free, spawn on the pinned port), because supersession cannot
  reuse a port the old daemon still holds; trigger is version skew after every upgrade,
  and `status`, which already diagnoses the skew, names the cure. Plus louder store
  identity in CLI output, finishing what the global `--project` flag started: two live
  cross-project misfires argued this in. Drill decisions (thread 74): the reachability
  probe runs *before* the kill (a sandboxed restart refuses with the daemon untouched);
  no browser open — tabs reconnect to the same port; honest bail after 10s, never
  SIGKILL; the identity line rides every write and only writes; and **restart is the
  user's command, never the agent's** — the skill says so.
- **A second embedded skill: `sideview-grill`.** ✓ **Accepted by the author,
  2026-08-22, thread 88 (drill round 17, all as suggested).** Drill decisions: the
  two skills install and report as one set (no per-skill flags; status is "current"
  only when both match); the ritual's final confirmation runs through the machinery —
  a one-close-block round — not chat; acceptance on the text, the first real grill
  arriving with a real topic. The grilling ritual (design tree,
  frontier, rounds with recommended answers) run through the page, shipped beside the
  main skill and installed alongside it by `skill install`. Separate on purpose: the
  main skill is model-invoked and loads constantly, the ritual is user-invoked and
  optional, so merging would bloat the core skill's context for every table and plan
  (the v0 dogfooding lesson). Self-contained about the ritual — no Skill-tool
  composition, which only Claude Code can do — deferring to the main skill for the
  medium. **Sequenced after the ask block**, which carries the round conventions the
  skill would otherwise have to teach in prose. Derived with credit from
  mattpocock/skills (MIT; the README invites exactly this). *(added by the author,
  2026-08-20, from the vision grill's own workflow)*
- **The logo, wired in — and it replaces the state dot** (author, 2026-08-20, thread 64
  on this page). ✓ **Accepted by the author, 2026-08-21, thread 76 (drill rounds
  9–10).** `logo.svg` (the reviewer: specs glancing right, one brow soft-raised;
  chosen through seven live iterations on the grill page) becomes the page favicon and
  the header mark beside the wordmark, inlined so `currentColor` follows the theme. The
  mark *is* the liveness indicator: the dot's existing client state machine toggles a
  class on the same inline SVG — open and glancing = live, half-lidded = reconnecting,
  lids drawn and pupils hidden = daemon gone. One indicator instead of two, no color
  needed. Animation (a blink on reconnect) deliberately deferred: charming, and scope
  creep. Drill decisions (threads 75–76): "gone" is honestly a duration — lids draw
  after 8s of failed retries; the favicon follows the OS scheme in the ink blues (it
  can't see the page's theme override); and closed eyes rest symmetric — the raised
  brow is a live expression, swapped for a resting brow element when the lids draw.
- **Cell marks — shipped as `_sv_mark_<col>`.** ✓ **Accepted by the author,
  2026-08-22, thread 91 (drill rounds 18–20).** The second half of the annotated-CSV
  design: cell-level tints beside the shipped `_sv_row`, the author's daily
  data-diff case — a deeper wash of the row duotone (cell 26% ink vs row 12%).
  The open aliasing question got answered properly after the author corrected a bad
  fact-check (the vocabulary lives in querier's AGENTS.md, not the sqlnow-mcp repo):
  sqlnow reserves `_sqlnow_` with `format_` (styles; added/changed/removed) and
  `cell_` (rich JSON widgets) — so the native name became `mark`, killing the
  false friend, `_sqlnow_*` columns hide here, and `_sqlnow_format_<col>` renders
  as marks with both vocabularies accepted in both directives: one annotated file
  renders in both viewers. Diff shapes proved script-side (round 19's rider), no
  new machinery: `examples/csvdiff.py --style marks|inline|split` — new-values,
  git-inline (`old -> new` in the changed cell), and side-by-side old/new pairs
  (old del, new add) — all three rendered live from the same sources during the
  drill. Bad directive names are silent no-ops, like an out-of-range freeze.
</sv-prose>

<sv-prose id="goal">
## Goal

v4 is done when each In item above is accepted by the author through this page's own
machinery, and **0.4.0 is on crates.io**. Blessed by the author 2026-08-20 (thread 66),
with a working order and a ritual attached: **the ask block ships first**, and when an
In bullet completes, the agent drills the author — an sv-ask round over whatever the
implementation did that the plan did not predict — on this page or a sibling. The
machinery reviews itself. *(It did: the ask block's own acceptance took seven rounds
through the ask block, threads 67–73 — and every other bullet followed the same
road: six bullets, twenty rounds, threads 67–91.)*
</sv-prose>

<sv-ask id="release" round="21" role="close">
**The release round.** All six In items are accepted; the binary says 0.4.0 and the
suite is green. **Approved** here is the publish order — v2's precedent, the release
line signing itself: I tag v0.4.0 (the workflow builds the binaries) and publish to
crates.io. **Revise** holds the release; say why in the note.
</sv-ask>

</sv-page>

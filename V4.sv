<sv-page label="v4: the page asks back" category="shipped" order="6">

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

✓ **Done, 2026-08-23: 0.4.0 is on crates.io, tag v0.4.0 pushed, the release ordered
through this page's own release round (thread 111) — the done-when below met exactly
as written.**

v4 is done when each In item above is accepted by the author through this page's own
machinery, and **0.4.0 is on crates.io**. Blessed by the author 2026-08-20 (thread 66),
with a working order and a ritual attached: **the ask block ships first**, and when an
In bullet completes, the agent drills the author — an sv-ask round over whatever the
implementation did that the plan did not predict — on this page or a sibling. The
machinery reviews itself. *(It did: the ask block's own acceptance took seven rounds
through the ask block, threads 67–73 — and every other bullet followed the same
road: six bullets, twenty rounds, threads 67–91.)*

Release frontier, reopened by round 21: the comment transport worked, but the harness
lifecycle did not. Claude Code has carried a chat-silent `watch` loop in this repo;
this Codex/API session instead buffered the live event until a new chat turn caused the
watcher PTY to be polled. The OpenCode and Pi matrix proved page authoring and daemon
access, not watcher-driven wake-up. **Round 22 expanded v4 to include the harness
lifecycle before release** (2026-08-22, thread 93), beginning with a Codex experiment.
Official OpenAI guidance identifies a durable goal as the native way for Codex to keep
working across turns toward a verifiable condition; the active experiment's condition
is deliberately visible: a submitted ask round must cause the next round to appear on
this page with no chat prompt. **The Codex leg passed in round 23** (2026-08-22,
thread 94): its answer arrived through `sideview watch` during an automatic durable-goal
continuation, and round 24 below was appended without a new chat message. The division
of labour is now measured: the goal keeps Codex taking turns; `watch` blocks within a
turn and delivers the page event. **Round 24 kept the harness branch open** (2026-08-22,
thread 95): once asked to watch, a harness should keep watching until explicitly told
to stop; completing a one-event proof is not the lifecycle contract. **Round 25 kept
one canonical skill with conditional harness recipes** (2026-08-22, thread 96), and
set the release evidence bar: Claude Code and Codex may document only their proven
paths; OpenCode and Pi need focused chat-silent wake tests before release. The standing
Codex goal then exposed its own UX cost live: the app continuously flashes working and
interactive prompts incur a small interruption delay. That rules out an indefinite
durable goal as the finished Codex recipe even though it proved the mechanism.
**Round 26 selected an event-driven Codex App Server bridge for experiment** (2026-08-22,
thread 97, all as suggested): a lightweight `sideview watch` process owns the idle
wait, then resumes one persistent project watcher thread and starts a Codex turn only
after an event arrives. The durable goal was stopped before the bridge test. This
keeps watcher context across comments without keeping this interactive chat in a
working state. **Round 27 exposed a bridge protocol fault** (2026-08-22, thread 98,
all as suggested; rider: "lets test"): Sideview delivered event 275 immediately, but
`turn/start` rejected the prototype's kebab-case sandbox-policy value — that field
uses App Server's camel-case `workspaceWrite`, unlike `thread/start`'s
`workspace-write`. The bridge retained cursor 274, so diagnosis could correct the
request and replay the unconsumed event; the repaired turn then folded this answer and
appended round 28. Because that repair required a chat prompt, it is recovery evidence,
not the requested chat-silent proof. **Round 28 supplied a clean proof of quiet
background delivery** (2026-08-22,
thread 99, all as suggested): with the corrected bridge already idle, event 276 woke
the persistent watcher thread and started this turn without a chat prompt or durable
goal; folding the answer here and appending round 29 is the visible acceptance signal.
That architecture worked, but its output lived in a separate Codex thread and was
therefore invisible in this conversation. **Round 29 rejected the hidden bridge and
selected the queue path** (2026-08-22, thread 100): the App Server bridge was stopped
while handling event 277, before its cursor advanced, and that same preserved event was
sent with `codex queue --thread <this-chat>`. The resulting turn appeared visibly in
this conversation: no durable goal and no hidden worker thread. Its first ownership
answer proposed a user-maintained supervisor script, but the author's immediate
clarification rejected that as product UX: one Sideview command should encapsulate the
watch, cursor, prompt, and queue handoff. Repository inspection supports a distinct
`sideview monitor codex` surface: existing `watch` is deliberately a pure,
sandbox-compatible JSON-lines reader, while monitor is side-effectful and
harness-specific. Sideview would own the implementation; the user would only opt into
its lifecycle. **Round 30 selected `sideview monitor codex` as that public boundary**
(2026-08-22, thread 102, all as suggested; rider: "Do you think having a single
command like this is the best solution?"). Yes: one product entry point is the best
user-facing solution because thread binding, cursor durability, delivery semantics,
and restart recovery are correctness-sensitive plumbing that should not be
reimplemented in personal wrapper scripts. "One command" does not mean one
monolithic primitive: `sideview watch` remains a pure JSON-lines stream, while the
monitor composes it with Codex queue delivery and exposes explicit lifecycle and
status controls. **Round 31 kept the exact-thread rule but qualified its environment
input** (2026-08-22, thread 103; selected suggestion; rider: "does the
CODEX_THREAD_ID exist in the cli as well"). In the installed `codex-cli 0.149.0`,
this Codex task exports `CODEX_THREAD_ID`, and the native CLI binary contains both
`CODEX_THREAD_ID` and `CODEX_SESSION_ID`; however, official OpenAI documentation does
not publish `CODEX_THREAD_ID` as a stable CLI contract. The monitor may therefore use
it as a convenience when present, but `--thread <id>` is the explicit supported input;
the resolved UUID is persisted and the command refuses ambiguity rather than guessing
"latest". **Round 32 accepted foreground-by-default plus explicit detach and authorized
the proper project implementation** (2026-08-22, thread 104, all as suggested; note:
"happy for you do do project modification and do the watcher properly now"). The new
`sideview monitor codex` keeps `watch` pure while owning exact thread and executable
discovery, a project-local JSON cursor/delivery record, a lifetime flock, PID start-token
identity, readable queue prompts, agent-echo filtering, post-delivery receipts, retry
without cursor advance, and `monitor status` / identity-checked `monitor stop`. Foreground
is default; `--detach` starts an internal session leader and waits for its state+lock
readiness. Five focused monitor tests join the full green suite (**76 tests**), and a
temporary-project lifecycle test proved detached start → running status → verified stop →
stopped status. The shell prototype was then stopped and the built product monitor took
over this exact ChatGPT desktop conversation using the desktop-bundled Codex binary:
PID 1069591, target `01a028f9-09d4-7380-b80a-6e57a287fb45`, resumed cursor 287 after
the final lifecycle-race hardening restart. **Round 33 selected at-least-once delivery**
(2026-08-22, thread 105, all as suggested; note: "a test of new monitor"): a rare replay
after the ambiguous external-queue crash window is preferable to silently losing human
feedback, and a retained pending key stays visible in `monitor status`. Event 288 was the
live end-to-end test of the product monitor itself: it was detected, formatted, queued into
this same desktop conversation, acknowledged only after Codex accepted it, and advanced the
durable cursor to 288 with no pending event or error. This completed the Codex monitor
hardening pass, but the desktop conversation stopped before asking for the ritual's distinct
final author confirmation. **Round 34 accepted the Codex monitor addition** (2026-08-22,
thread 106; approved with the condition "only if the monitor works to pick this up"). Event
290 was picked up by the installed product monitor and queued into this exact CLI thread
without another chat prompt, satisfying that condition and closing the Codex harness branch.
The settled command, lifecycle, and delivery-policy decisions remain unchanged. Publication
remains outside the experiment.
</sv-prose>

<sv-prose id="bridge-model">
## What the Codex queue monitor connects

| Surface | Role | What you see |
|---|---|---|
| **Sideview browser** | Sends a round as a Sideview event | The round folds; later, a new round appears |
| **`sideview monitor codex`** | Encapsulates `sideview watch`, cursor state, and `codex queue` | One explicit command; idle means no model turn |
| **This interactive Codex thread** | Receives the event as its next user turn, edits `V4.sv`, and answers | The turn appears here and its edits appear in Sideview |

The path is **Sideview → `sideview monitor codex` → `codex queue` → this thread →
`V4.sv` → Sideview**. Unlike the durable goal, no Codex turn exists while the
monitor waits. Unlike the App Server prototype, the work and final response are
visible in this conversation. Event 277 took this path to produce the current turn.

| Client surface | Queue-monitor expectation |
|---|---|
| **Codex CLI queue transport** | Proven: event 277 appeared as a turn in the targeted conversation |
| **Codex in the ChatGPT desktop app (this environment)** | **Proven:** events from the live Sideview watcher were queued into this same visible desktop conversation and started new turns |
| **Local IDE client** | Unverified: plausible through local Codex session machinery, but needs its own focused test |
| **Ordinary ChatGPT web or mobile chat** | Out of scope: the local `codex queue` command cannot address an arbitrary remote ChatGPT conversation |

**Codex in the ChatGPT desktop app (which is what we are running in) is now proven.**
The evidence covers both local `codex queue` delivery and visible turn creation in the
desktop conversation. It does not establish IDE, web, or mobile behavior: each client
surface must still prove that its visible thread can be targeted, and the packaged
command should detect and refuse unsupported surfaces rather than infer compatibility.

**Codex executable discovery.** The monitor should make a best-effort search for a
compatible Codex command: prefer an explicit executable path, then `codex` on `PATH`,
then known desktop-bundled locations for the current platform. Every candidate must be
preflighted for a compatible `queue` command before use. If discovery fails, stop with
an actionable diagnostic that lists what was checked and advises the user or agent to
point the monitor at an existing compatible binary or install the official Codex CLI.
Do not silently install software, guess an unrelated executable, or hard-code one
platform's desktop path as the universal contract.
</sv-prose>

<sv-ask id="release" round="21" role="close">
**The release round.** All six In items are accepted; the binary says 0.4.0 and the
suite is green. **Approved** here is the publish order — v2's precedent, the release
line signing itself: I tag v0.4.0 (the workflow builds the binaries) and publish to
crates.io. **Revise** holds the release; say why in the note.
</sv-ask>

<sv-ask id="d22q1" round="22">
**The harness boundary.** `sideview watch` delivered round 21 immediately, but this
Codex/API harness cannot wake an idle model turn from background PTY output. Where does
that discovery land?
- * Keep v4 scoped: document that chat-silent continuation is harness-dependent, then return to release approval
- Expand v4: do not release until Codex, OpenCode and Pi can all continue from a watch event without a chat prompt
- Run a focused OpenCode/Pi/Codex wake matrix before choosing the release boundary
</sv-ask>

<sv-ask id="d22fin" round="22" role="close">
**Round 22: watcher delivery versus harness wake-up.** Send this boundary decision;
the next round will follow the branch you choose.
</sv-ask>

<sv-ask id="d23q1" round="23">
**The Codex goal test.** A durable Codex goal is now active for this thread, with the
watch-and-grill loop as its objective. What is the acceptance signal?
- * After this send, round 24 appears here without any new chat message
- A hidden reply on this round's machine thread is enough
- Merely seeing this submission marked sent is enough
</sv-ask>

<sv-ask id="d23fin" round="23" role="close">
**Round 23: chat-silent Codex continuation.** Send this round, then do not prompt in
chat. The test passes only if the page itself visibly advances.
</sv-ask>

<sv-ask id="d24fin" round="24" role="close">
**Round 24: the visible proof.** This block was appended after round 23 by an automatic
Codex durable-goal continuation, with the answer delivered through `sideview watch` and
no chat prompt. **Approved** accepts the Codex leg as proven; **revise** names any part
of the behavior that still does not meet the intended feedback loop.
</sv-ask>

<sv-ask id="d25q1" round="25">
**Skill shape.** Today `sideview skill install` writes the same embedded skill bytes to
Claude Code, Codex, OpenCode and Pi, and `status` checks every copy against that one
source. How should harness lifecycle differences be taught?
- * Keep one canonical skill with a short conditional section per harness; the Sideview contract stays shared while only the wait/continuation recipe varies
- Build four harness-specific skill variants, each free to describe its own watcher lifecycle
- Keep the skill generic and leave persistent watching entirely to harness documentation
</sv-ask>

<sv-ask id="d25q2" round="25">
**Evidence before release.** Claude Code's blocking watcher and Codex's durable-goal
watcher are now proven here; the old OpenCode/Pi matrix tested authoring and daemon
access, not chat-silent continuation. What should v4 require?
- * Document the two proven recipes, label OpenCode/Pi unverified, and run their focused wake tests before release
- Publish a generic blocking-watch recipe for OpenCode/Pi without another matrix
- Support persistent watching only in Claude Code and Codex for v4
</sv-ask>

<sv-ask id="d25fin" round="25" role="close">
**Round 25: one contract, four harness lifecycles.** Send the documentation shape and
evidence bar; the next frontier will specify the chosen recipes and tests.
</sv-ask>

<sv-ask id="d26q1" round="26">
**Codex while idle.** The finished watcher should react quickly without leaving this
chat visibly working or delaying ordinary prompts. Which architecture should v4 pursue?
- * Event-driven App Server bridge: a lightweight `sideview watch` process blocks with no model turn, then resumes one persistent project watcher thread and starts a Codex turn only when an event arrives
- Background scheduled task: poll Sideview on a cadence in a dedicated chat, accepting delayed comments
- Standing durable goal in this chat: keep today's proven path despite the continuous working state and prompt interruption
</sv-ask>

<sv-ask id="d26fin" round="26" role="close">
**Round 26: quiet until an event.** Send the Codex wake architecture; the next frontier
will settle ownership, lifecycle, and safety for the selected path.
</sv-ask>

<sv-ask id="d27q1" round="27">
**Event-bridge proof.** The durable goal is stopped, and an experimental
Sideview-to-Codex App Server bridge is now waiting without an active model turn. What
must count as acceptance?
- * After this send, round 28 appears here without any chat prompt and without reactivating a durable goal
- A bridge log showing that a Codex turn started is sufficient even if this page does not advance
- Accept the architecture from its idle behavior now and defer event delivery to implementation
</sv-ask>

<sv-ask id="d27fin" round="27" role="close">
**Round 27: event-triggered Codex.** Send this, then stay out of chat. The proof passes
only when the separate watcher thread folds your answer into this plan and visibly
appends round 28.
</sv-ask>

<sv-ask id="d28q1" round="28">
**Corrected bridge retest.** Event 275 was preserved and replayed successfully after
fixing the rejected App Server sandbox-policy value, but diagnosis required a chat
prompt. What now counts as the clean proof?
- * After this send, round 29 appears here without any chat prompt and without reactivating a durable goal
- Treat the repaired replay of round 27 as sufficient and proceed to hardening
- Require two consecutive chat-silent rounds before proceeding
</sv-ask>

<sv-ask id="d28fin" round="28" role="close">
**Round 28: clean event-trigger retest.** Send this, then stay out of chat. The test
passes only if the already-running corrected bridge visibly appends round 29.
</sv-ask>

<sv-ask id="d29q1" round="29">
**Bridge ownership.** The proven prototype holds a Sideview event cursor and one Codex
watcher-thread identity for this project, then invokes Codex App Server only when an
event arrives. Which product should own that long-lived boundary?
- * Sideview owns one explicit bridge per project, keeping its cursor and Codex thread identity under `.sideview`; Codex remains the invoked harness, not the supervisor
- Codex owns one global bridge that discovers and multiplexes every Sideview project
- Neither product owns it; keep the bridge as a user-maintained supervisor script
</sv-ask>

<sv-ask id="d29fin" round="29" role="close">
**Round 29: ownership of the proven bridge.** Send the ownership boundary; the next
frontier will derive its process identity, CLI lifecycle, concurrency, and failure
policy rather than assuming them across incompatible owners.
</sv-ask>

<sv-ask id="d30q1" round="30">
**One-command Codex monitor.** The current proof is a small wrapper around `watch` and
`codex queue`. Where should that behavior live for users?
- * Add `sideview monitor codex`: one explicit, harness-specific command owns thread binding, cursor persistence, queue delivery, status, and stop; keep `sideview watch` as pure JSON-lines
- Add Codex flags directly to `sideview watch`, making watch both an event stream and an action runner
- Keep the wrapper as a documented external script that every user maintains
</sv-ask>

<sv-ask id="d30fin" round="30" role="close">
**Round 30: make the monitor a product command.** Send the command boundary; the next
frontier will derive thread selection, foreground/detached lifecycle, delivery
acknowledgement, and restart recovery for the chosen surface.
</sv-ask>

<sv-ask id="d31q1" round="31">
**Target-thread binding.** Which Codex conversation should `sideview monitor codex`
queue each event into?
- * Use `CODEX_THREAD_ID` when launched from Codex, accept an explicit `--thread <id>` elsewhere, persist the resolved UUID, and refuse ambiguous or missing targets; never guess "latest"
- Always require `--thread <id>`, even when Codex already provides the current thread identity
- Discover the most recently active Codex thread for the project and follow it as activity changes
</sv-ask>

<sv-ask id="d31fin" round="31" role="close">
**Round 31: bind the visible conversation.** Send the target rule; the next frontier
will choose foreground versus detached lifecycle and define status/stop behavior.
</sv-ask>

<sv-ask id="d32q1" round="32">
**Monitor lifecycle.** How should the single Codex-monitor surface run and stop?
- * Run in the foreground by default; make `--detach` explicit; persist project-local PID, cursor, target UUID, and delivery state; provide exact `status` and `stop` operations that detect stale ownership without killing unrelated processes
- Detach by default and require `sideview monitor codex stop` to end it
- Support foreground operation only and leave persistence/restart to an external process supervisor
</sv-ask>

<sv-ask id="d32fin" round="32" role="close">
**Round 32: own the process lifecycle.** Send the lifecycle rule; the next frontier
will define when an event is acknowledged, retried, or quarantined after queue failure.
</sv-ask>

<sv-ask id="d33q1" round="33">
**Queue delivery bias.** The monitor records a pending event before invoking `codex queue`,
advances its cursor and stamps the Sideview receipt only after exit 0, and retries ordinary
failures with capped exponential backoff. No local transaction can be atomic with the
external Codex queue: a process crash after Codex accepts the message but before the cursor
write can replay that one event. Which failure bias should ship?
- * Keep at-least-once delivery: a rare duplicate after an ambiguous crash is recoverable, while silently losing human feedback is not; retain the pending key and expose it in `monitor status`
- Mark the cursor before invoking Codex for at-most-once delivery, accepting that a crash can silently lose the event
- Quarantine every pending event found after restart and require a manual retry/skip decision before monitoring continues
</sv-ask>

<sv-ask id="d33fin" round="33" role="close">
**Round 33 accepted: keep at-least-once delivery.** Event 288 also completed the live
end-to-end test of the product monitor in this desktop conversation.
</sv-ask>

<sv-ask id="d34fin" round="34" role="close">
**Round 34: final Codex monitor acceptance.** The product command, exact-thread binding,
foreground/detached lifecycle, durable at-least-once delivery, CLI and desktop wake proofs,
and installed skill instructions are now in place. **Approved** accepts the Codex monitor
addition and closes this harness branch. **Revise** keeps it open; name what still needs work
in the note.
</sv-ask>

<sv-prose id="opencode-monitor-proof">
## OpenCode personal monitor proof

The focused OpenCode test now has the evidence the earlier harness matrix lacked. A
separate `sideview-opencode` instruction skill led a fresh agent to build a personal,
global OpenCode plugin against the installed SDK rather than adding a Sideview-owned
package. Its explicit start tool bound the exact OpenCode session and worktree; Sideview
comment events 292, 294, 297, 298 and 304 each started visible turns in that same idle
conversation without another chat prompt. Agent replies were filtered without echoing,
status retained the last event and errors, and event 304 proved the shortened prompt shape.

The boundary stays deliberately smaller than the Codex product monitor: in-memory state,
one OpenCode process and one owner per worktree, no historical replay, no detached process,
and a documented startup-baseline window. The generated plugin is personal configuration;
Sideview maintains the build instructions, not a published OpenCode plugin.

**Accepted in round 35, 2026-08-22 (thread 108), all as suggested.** The personal-plugin
recipe is the proven OpenCode wake path; its instruction skill stays separate and
uninstalled for now, and Sideview does not take on a maintained OpenCode plugin. The
acceptance round itself returned through the compact monitor without a chat prompt, so
the grill path passed as part of the sign-off.
</sv-prose>

<sv-ask id="d35q1" round="35">
**OpenCode completion boundary.** What should these live results close?
- * Accept the personal-plugin recipe as the proven OpenCode wake path, keep the instruction skill separate and uninstalled for now, and do not create a Sideview-maintained plugin
- Keep the OpenCode branch open until busy-session, burst, and multi-process tests are complete
- Turn the experiment into a published Sideview OpenCode plugin now
</sv-ask>

<sv-ask id="d35fin" round="35" role="close">
**Round 35: OpenCode personal monitor acceptance.** Send **approved** to mark this
experiment complete at the boundary above. Send **revise** to keep it open and name the
missing proof in the note. This submission is also the grill-path test: the running
OpenCode monitor should carry the round back into this same conversation without a chat
prompt.
</sv-ask>

<sv-prose id="pi-watch-recipe-grill">
## Pi watcher recipe accepted

Design tree for accepting the Pi watcher skill:

- Pi integration surface
  - Settled: Sideview should not yet ship a maintained Pi extension package.
  - Settled: agents should generate a project-local `.pi/extensions/sideview-watch.ts` from a recipe.
  - Settled: the watcher is resident extension code, not background bash.
  - Settled: agent control is first-class in the recipe: slash commands are for humans; model-callable tools are for agents.
  - Settled: the live proof is enough to install and dogfood the skill.
- Watch ownership
  - Settled: project-wide steward claiming is common and valid when explicit.
  - Settled: page-scoped claiming needs a real `sideview watch --page` filter before it can be exactly-once.
- Lifecycle
  - Settled: `session_start` may autostart when configured.
  - Settled: `session_shutdown` always stops the child.
  - Settled: `sideview_watch_start`, `sideview_watch_stop`, and `sideview_watch_status` tools are required when the agent should own start/stop.

**Accepted in round 36, 2026-08-22 (thread 110), all as suggested.** The author's note made the condition explicit: approve only if the close event reaches Pi. It did — event 314 arrived through the tool-started project-wide watcher, so the condition is met. The `sideview-pi-watch` recipe skill is accepted for installation and dogfooding, with the remaining limitation recorded: safe page-scoped claiming awaits a real page filter.
</sv-prose>

<sv-ask id="d36q1" round="36">
**Agent-operable Pi watcher.** The first generated Pi extension exposed only slash commands, and this live session proved an agent cannot invoke those commands for itself. What should the recipe require?
- * Require both human slash commands and model-callable tools (`sideview_watch_start`, `sideview_watch_stop`, `sideview_watch_status`) sharing the same helpers; autostart remains optional
- Keep slash commands only and document that the user must start/stop the watcher manually
- Require autostart only and remove manual controls from the recipe
</sv-ask>

<sv-ask id="d36q2" round="36">
**Acceptance proof for the Pi recipe skill.** The tool-backed watcher was started from this agent session, then delivered repeated Sideview comments, cross-thread comments, a resolution event, and an older unclaimed event 307 through `--since 0 --claim --ack`. What should that close?
- * Accept the `sideview-pi-watch` recipe skill as good enough to install and dogfood, with a note that safe page-scoped claiming still awaits a real page filter
- Keep the skill open until a separate subagent independently regenerates the extension from scratch
- Turn the recipe into a maintained Sideview Pi extension package now
</sv-ask>

<sv-ask id="d36fin" round="36" role="close">
**Round 36: Pi watcher recipe skill acceptance.** Send **approved** to accept the
recipe boundary above and close the Pi watcher skill test. Send **revise** to keep it
open and name the missing proof in the note.
</sv-ask>

<sv-ask id="release2" round="37" role="close">
**The release round, resumed.** Round 21's hold is answered: v4 grew the harness
bar and met it — Codex wakes through `sideview monitor codex`, OpenCode through the
personal-plugin recipe (round 35), Pi through the generated extension recipe
(round 36), each continuing from a watch event with no chat prompt. All of it is
committed, the suite is green at 76, the binary says 0.4.0, and every skill is
current on all four harnesses. **Approved** is the publish order: I tag v0.4.0
(release.yml builds the binaries) and publish to crates.io. **Revise** holds it;
say why in the note.
</sv-ask>

</sv-page>

---
name: sideview
description: Show the user rich visual content — plans, HTML, diagrams-in-markup, screenshots, anything too wide or too visual for a terminal — on a live browser page by piping it to the `sideview` CLI. Use whenever presenting a plan or design argument, comparing options, or showing an image.
---

# sideview — a visual side channel

Anything you would render as a wall of terminal markdown can appear on a styled, live
browser page instead. The page is already open on the user's side; blocks you write
appear on it within ~100ms. Reach for it when output is **visual** (images, layout,
tables), **long-form** (plans, design arguments, comparisons), or **revisable**
(status you'll update as you work).

## The five commands

Content always arrives on stdin. Every authoring command prints the block's id alone
on stdout; keep it if you may revise the block.

```bash
sideview prose <<'EOF'      # markdown (GFM: tables, task lists, strikethrough)
## The plan
1. Parse the 2024 file
EOF
# → b1

sideview markup <<'EOF'     # an HTML fragment, styled by the page
<div class="alert alert-warning">4% of rows drop here</div>
EOF
# → b2

sideview html < page.html   # a whole HTML document, isolated in an iframe
                            # (--height 40rem sizes the frame; viewers can drag it)

git diff | sideview diff    # a unified diff, rendered properly — file paths
                            # become outline sections; the viewer can switch
                            # unified ⇄ side-by-side. Pipe straight from git:
                            # the diff never needs to enter your context.

sideview update b1 <<'EOF'  # replace a block in place — the page patches, nothing reflows
## The plan (revised)
EOF

sideview rm b2              # remove a block
```

Ids are per-page: you can only ever name your own page's blocks, so use them freely.
Prefer `update` over appending corrections — plans are revised constantly, and a
corrected block reads better than a correction below a mistake.

**Your page is a file, and editing it directly is equivalent to the CLI.**
Everything you write lands in `.sideview/pages/<page>.sv`: blocks fenced by
`<sv-prose|markup|html id="bN">` tags whose bodies are raw bytes — never
HTML-escape anything inside a block. String-edit a block's text and the page
patches within a tick; small revisions are often easier this way than
re-piping a whole block through `update`. Tags count only at column 0 (indent
by one space to show a literal tag); keep the closing `</sv-...>` lines intact.

**Prefer `prose` over `markup`.** Markdown is cheaper to write, and the page styles
it fully — headings, tables, task lists, code. Reach for `markup` only when you need
components or layout markdown can't express (cards, grids, badges, metrics).

**The page is live — stream it.** Write blocks as you go rather than composing the
whole page before emitting the first one; an early skeleton that sharpens through
`update` beats a long silence and a reveal. The reader is watching from block one.

**Diagrams are optional, and for a working plan usually skippable** — prose and
tables are faster to write and faster to read; don't let a diagram slow the plan
down. There is no mermaid: a ```mermaid fence renders as a plain code block
(heavyweight renderers belong to the extension layer). When a picture genuinely
helps, hand-author inline SVG in a prose or markup block, and do it properly: native shapes and `<text>`, size with `viewBox`, strokes
and text in `currentColor` with at most one literal accent on the element that
carries the meaning, label the arrows (`writes`, `polls every 30s` — an unlabeled
arrow is just "related somehow"), wrap in `<figure>` with a `<figcaption>` stating
the claim, and depict the mechanism, not its name. Never build diagrams out of
divs and CSS.

Optionally, set page properties (no ids needed — it applies to your own page):

```bash
sideview page set --label "Parser fix plan"      # names the page in the header strip
sideview page set --outline tabs                 # sections become separate panes
sideview page set --outline off                  # no contents rail on this page
sideview page rm                                 # delete your page, file and all (rarely needed)
```

Pages with sections get a contents rail automatically, built from your blocks'
headings; by default the whole page scrolls and the rail follows (scrollspy).
Set `--outline tabs` when sections are alternatives rather than a sequence —
option A/B comparisons, multiple prototypes. Set `--outline off` when the page
isn't a document at all — a dashboard, a single answer, a screenshot gallery.

Text sits at a comfortable reading width; tables, code blocks and figures widen
automatically. To give a markup block the full wide column (a dashboard row, an
embedded document), put `class="w-100"` on its top-level element.

## Tables: reference a CSV, never paste one

Produce the file with whatever tool you have, then add an `sv-csv` block to
your page file (a string edit — see "your page is a file" above):

```bash
duckdb -csv -c "select ..." > out/rows.csv    # or sqlnow, sqlite3, pandas…
```

```
<sv-csv id="t1" src="out/rows.csv" freeze="2" height="24rem">
</sv-csv>
```

The daemon reads the file and renders a real table: sticky header always,
`freeze="N"` pins the first N columns (1–4) through horizontal scroll,
`height` bounds the block with its own scroll. Capped at 2,000 rows with an
honest remainder line — past that a human cannot review it; query a subset
instead. **Overwrite the file and the block re-renders**, so re-running your
analysis updates the page with no further writes. The rows never pass
through your context: keep the query's output in the file, not in your reply.

For a data diff, compute the comparison yourself and add a `_sv_row` column
with `add` / `del` / `mod`: rows tint like a diff, and `_sv_*` columns never
display. A `_sv_mark_<col>` column marks single cells the same way — the
usual shape is a `mod` row with the changed cells marked deeper. The marks
don't care what the cell text says, so richer diff shapes are just output
shapes: put `old → new` in a marked cell for a git-style inline diff, or
emit old/new column pairs (old marked `del`, new `add` where changed) for
side-by-side. sqlnow's directives are honored too: `_sqlnow_format_<col>`
values `added`/`changed`/`removed` render as the same tints and every
`_sqlnow_*` column hides — one annotated file renders in both viewers.
Paths are project-relative.

## Ask blocks: questions answered at the block

When you need decisions from the reader, don't bury questions in a comment —
put `sv-ask` blocks on the page (a string edit, like `sv-csv`). One block per
question: every column-0 `- ` line becomes a selectable option
(single-choice; `pick="many"` on the tag makes them multi-select), a `- * `
line is your suggested answer — badged and pre-selected, so an untouched
question visibly submits it — the rest of the body is the question
(markdown), and every question carries a free-text field alongside. Suggest
an answer whenever you have one. Group a round with `round="N"` and end it
with one close block:

```
<sv-ask id="q1" round="1">
Which retry policy?
- * exponential backoff
- fixed 5s
</sv-ask>

<sv-ask id="fin1" round="1" role="close">
</sv-ask>
```

The close block renders the round's verdict (approved / revise), an optional
closing note, and the send button. A round can be a
lone close block when the verdict is the whole question. Answers stay
drafts in the reader's browser until they
press send, which posts **one comment for the whole round**: it arrives
through `sideview watch` like any comment, on a thread on the close block,
first line `ask round N — <verdict>`, then a terse list keyed by question
block id: only questions whose answer differs from your suggestion appear
(`q2: <picks>`, riders quoted beneath, unanswered ones say so), and `all as
suggested` / `everything else as suggested` covers the rest. A sent round
folds to one line on the page but never locks: an amended send arrives as a
reply on the same thread. Round threads are yours alone — the page's comment
bar never shows them — so after folding the answers into the page (update
the blocks the round settles), resolve the round's thread yourself: the
reader has no UI for it. A new round of follow-ups is new `sv-ask` blocks
with the next `round` number, never edits to a sent round's blocks.

## The page is styled with Bootstrap 5 — write what you already know

Markup blocks can use the full Bootstrap 5 vocabulary: grid (`row`/`col-*`),
utilities (`d-flex`, `gap-3`, `mt-4`, `text-muted`), and components (`alert
alert-warning`, `card`, `badge text-bg-danger`, `table`, `progress`). Do not
hand-roll CSS or inline `style=` — reach for the Bootstrap class instead. CSS only:
components needing bootstrap.js (dropdowns, modals, collapse) won't behave; prefer
native `<details>`. Bare semantic elements — tables, blockquotes, `<figure>`,
`<kbd>` — are styled too, so markdown and plain HTML look right with no classes at
all. Light and dark both work automatically; never set colors directly. For the
small `sv-` layer (metrics, deltas), run `sideview styles`. Images: `<img
src="shots/before.png">` in a markup block — the path resolves against the project
root, so copy files into the project first (`$TMPDIR` gets cleaned up).

## Interactive blocks: an `html` block is a Vue island

When interaction *is* the point — a number the reader should push on, options
they want to sort, a matrix worth exploring — an `html` block can be a real Vue
app. Vue is vendored and mapped, so the import is the spelling you already know:

```html
<div id="app" class="p-3"></div>
<script type="module">
  import { createApp, ref, computed } from 'vue'
  createApp({
    setup() {
      const rate = ref(4)
      return { rate, dropped: computed(() => Math.round(4000 * rate.value / 100)) }
    },
    template: `
      <input class="form-range" type="range" min="0" max="25" v-model.number="rate">
      <div class="sv-metric">{{ dropped }} rows dropped</div>`,
  }).mount('#app')
</script>
```

No CDN, no build step, no `<script src>` juggling: write `from 'vue'` and it
resolves to the vendored copy. The block inherits the page's Bootstrap and design
system (so it looks native without CSS of yours), follows the theme toggle, and
autosizes — the frame grows to its content, so don't set `--height` unless you
want it pinned. Use the composition API with an explicit `setup()` and an inline
`template` string: `<script setup>` sugar needs a compiler and there is none.

An island cannot reach the page that hosts it (sandboxed, opaque origin), which
is what makes it safe to be careless in. Reach for one only when interaction is
the point — prose, tables and diffs are cheaper to write, cheaper to read, and
they survive in a snapshot where an island's state does not.

## The page talks back — comments, and who resolves them

**If the user asked for the page, watching is part of the ask.** Don't end at
the last block: their reply arrives *on* the page, so present, then run `watch`
and act on what comes back. A page delivered without a watcher is a question
with nobody listening for the answer. (`--timeout N` and re-arm if your
harness can't block open-endedly.)

For a Codex conversation that should wake only when feedback arrives, use the
product monitor instead of maintaining a shell wrapper:

```bash
sideview monitor codex                         # foreground; CODEX_THREAD_ID
sideview monitor codex --thread <uuid> --detach
sideview monitor status
sideview monitor stop
```

The monitor starts at the current latest event on first use, then persists its
exact target UUID and delivered cursor under this project's `.sideview/` so a
restart resumes without replaying history. It skips agent echoes, queues readable
event messages with `codex queue`, and acknowledges a comment only after queue
delivery succeeds. `--codex-path <path>` (or `SIDEVIEW_CODEX_PATH`) overrides
best-effort discovery; discovery checks `PATH` and known desktop bundles, verifies
the candidate has a compatible `queue` command, and otherwise tells the user or
agent how to point at or install the correct binary. Foreground is the default;
`--detach` is explicit, and `stop` verifies the recorded process identity before
signalling it. Keep raw `sideview watch` for other harnesses, custom consumers,
and cases where JSON-lines are the desired interface.

Readers comment by double-clicking any text bit (its text becomes the quote)
or selecting exact words and clicking the bubble chip; every conversation
lives in the right-hand comment bar (open threads as cards, resolved folded
beneath), never inline. Await their feedback with `sideview watch --since 0 --skip-author
agent --ack` — typed JSON-lines, one event per comment/resolve, your own echoes
filtered server-side. Add `--claim` only when several watchers split one
page's work and each event is acted on the moment it is read: claim couples
"seen" with "acted", and a claimed event lost in transit is invisible to
reach-back. Reply on the thread the event names:
`sideview comment --thread <id>` (body on stdin) — and pass `--page <page>`
from the same event as a guard: it refuses if the thread lives elsewhere,
which catches the classic agent hazard of a resetting shell resolving the
wrong project's store. For the same reason, prefer `--project <dir>` (or
SIDEVIEW_PROJECT) over relying on your cwd in multi-project sessions. Start your own
thread with `sideview comment <block> [--at <anchor>]`. Your comments carry
`author: "agent"`; a thread where the agent spoke last shows a filled bubble
on the page — that is the handoff.

**Answering a thread is not closing it — do not resolve just because you
replied.** Resolving moves the conversation off the page into the tail list,
which buries your answer at the exact moment the filled bubble was pointing
the reader at it. Threads are conversations, not tickets: the person whose
concern started the thread decides when it is settled. Reply, then stop.

`sideview resolve <thread>` is for when the user tells you — a direct ask, a
standing instruction ("resolve these once fixed"), or bulk cleanup they have
requested. `--undo` reopens anything resolved by mistake.

## The page edits back — two tiers on watch

Readers edit blocks from the page (select text, then the chip's pencil).
**Prose blocks splice straight into your page file** under the same lock your
writes take; watch tells you with a `kind: "edited"` comment naming the page
and block — re-read the file before your next `update` or `rm` there (block
ids are stable; only the text moved). **Every other block type arrives as an
edit request**: a `kind: "edit"` comment whose body is the proposed change
fenced in ```` ```md ````. Merge it into the block yourself, reply on the
thread, and resolve once merged — `working` covers the gap if it will take a
while. Ordinary comments carry `kind: "comment"`; `edited` records never
appear in the page's comment bar, so replying to one reaches nobody.

## If it says "no daemon running"

Your block is saved — nothing is lost — but no page is showing it. The daemon cannot
be started from inside the sandbox (its network namespace is unreachable). Offer to
run `sideview --detach` with the sandbox disabled; that is one approval and blocks
written so far appear the moment it starts. If declined, relay the printed
instruction (`run \`sideview\` in <project>`) to the user and continue.

**`sideview restart` is the user's command, never yours.** It kills the running
daemon, and that call belongs to the person whose tabs are on it. If you see
version skew or a stale daemon, say "run `sideview restart`" and keep working —
your writes land in the file either way.

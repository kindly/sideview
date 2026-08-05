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

git diff | sideview diff    # a unified diff, rendered properly — file paths
                            # become outline sections; the viewer can switch
                            # unified ⇄ side-by-side. Pipe straight from git:
                            # the diff never needs to enter your context.

sideview update b1 <<'EOF'  # replace a block in place — the page patches, nothing reflows
## The plan (revised)
EOF

sideview rm b2              # remove a block
```

Ids are per-session: you can only ever name your own blocks, so use them freely.
Prefer `update` over appending corrections — plans are revised constantly, and a
corrected block reads better than a correction below a mistake.

**Your page is a file, and editing it directly is equivalent to the CLI.**
Everything you write lands in `.sideview/pages/<session>.sv`: blocks fenced by
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
down. When a quick picture of a flow or sequence genuinely helps, a ```mermaid
fence in prose renders themed to the page. Reserve hand-authored inline SVG for
pages whose *point* is visual presentation — a guide, an overview, a showcase —
and then do it properly: native shapes and `<text>`, size with `viewBox`, strokes
and text in `currentColor` with at most one literal accent on the element that
carries the meaning, label the arrows (`writes`, `polls every 30s` — an unlabeled
arrow is just "related somehow"), wrap in `<figure>` with a `<figcaption>` stating
the claim, and depict the mechanism, not its name. Never build diagrams out of
divs and CSS.

Optionally, set page properties (no ids needed — it applies to your own page):

```bash
sideview session set --label "Parser fix plan"   # names the page in the header strip
sideview session set --outline tabs              # sections become separate panes
sideview session set --outline off               # no contents rail on this page
sideview session rm                              # delete your page, file and all (rarely needed)
```

Pages with sections get a contents rail automatically, built from your blocks'
headings; by default the whole page scrolls and the rail follows (scrollspy).
Set `--outline tabs` when sections are alternatives rather than a sequence —
option A/B comparisons, multiple prototypes. Set `--outline off` when the page
isn't a document at all — a dashboard, a single answer, a screenshot gallery.

Text sits at a comfortable reading width; tables, code blocks and figures widen
automatically. To give a markup block the full wide column (a dashboard row, an
embedded document), put `class="w-100"` on its top-level element.

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

## If it says "no daemon running"

Your block is saved — nothing is lost — but no page is showing it. The daemon cannot
be started from inside the sandbox (its network namespace is unreachable). Offer to
run `sideview --detach` with the sandbox disabled; that is one approval and blocks
written so far appear the moment it starts. If declined, relay the printed
instruction (`run \`sideview\` in <project>`) to the user and continue.

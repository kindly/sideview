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

sideview update b1 <<'EOF'  # replace a block in place — the page patches, nothing reflows
## The plan (revised)
EOF

sideview rm b2              # remove a block
```

Ids are per-session: you can only ever name your own blocks, so use them freely.
Prefer `update` over appending corrections — plans are revised constantly, and a
corrected block reads better than a correction below a mistake.

Optionally, set page properties (no ids needed — it applies to your own page):

```bash
sideview session set --label "Parser fix plan"   # names the page in the header strip
sideview session set --outline off               # no section side-menu on this page
```

Long documents get an outline sidebar automatically, built from your blocks'
headings. Set `--outline off` when the page isn't a document — a dashboard, a
single answer, a screenshot gallery.

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

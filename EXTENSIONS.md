# Sideview extensions

How to build an extension for sideview. This document is the contract: an
extension written against it should work without reading sideview's source.
It is honest about status — the mechanism below is **specified, not yet
built** (2026-08-15); it is published first so that an extension and the
mechanism can be built against each other and this document corrected where
it proves unclear. Design rationale lives in V3.sv's plugins section; this
file states only what is so.

## The idea in four sentences

A sideview **page** is a file of typed blocks, rendered live in the browser.
An **extension** adds a block type: it owns a tag (`<sv-table>`), and every
block with that tag renders as the extension's UI. An extension is a
**directory in the repo** — HTML, CSS, JS, and optionally a **binary** the
frame may invoke through a two-function API. Installing one is listing it in
the repo's config; installation is the trust act, and an installed extension
is fully trusted.

## Trust model, stated plainly

The model is VS Code's, not the web's. An installed extension runs
same-origin with the page, may load what it likes (including from a CDN),
and may invoke its declared binary with arguments of its choosing. Sideview
does not sandbox installed extensions — the `sandbox` attribute never
protected anyone from code they chose to install. What sideview does
guarantee is the inverse boundary: *content* (blocks agents write, pages
anyone edits) cannot reach extension machinery — see "What content cannot
do" below.

A consequence stated so it is chosen rather than stumbled into: an installed
extension wrapping `git` (or any tool with hook-like configuration —
`.gitattributes` textconv, for example) runs whatever that configuration
says. Core sideview refuses exactly this and uses gitoxide instead; an
extension may accept it, because installing the extension was the decision.


## Anatomy

```
extensions/
  table/
    plugin.toml        required — the manifest
    index.html         the entry, served into the block's frame
    app.js             yours; any files you like, served relative to entry
    style.css
```

### plugin.toml

```toml
name = "table"         # claims the block tag <sv-table>; [a-z0-9-], unique per repo
api = 1                # the version of THIS contract the extension expects
render = "frame"       # "inline" | "frame" | "sandbox" (sandbox: reserved, unbuilt)
entry = "index.html"   # relative to this directory
bin = "sqlnow"         # optional — the one binary call_cli may exec;
                       # a bare name resolves on PATH, "./…" resolves
                       # relative to this directory
```

Required: `name`, `api`, `render`, `entry`. A missing manifest means the
directory is not an extension — a stray `index.html` never becomes one by
accident. An `api` above what the running sideview knows, or an unknown
`render`, degrades to an honest "this sideview doesn't know…" block, never a
guess.

### Registration (the install)

In the repo's `.sideview.toml`:

```toml
[[extensions]]
path = "extensions/table"
# enabled = false      # present but off
```

Config lists installs; everything an extension can say about itself lives in
its own manifest. Distribution from outside the repo (npm, crates, URLs) is
deliberately out of scope for now: an extension is files in your repository.

## What a block looks like

```
<sv-table db="data/readings.duckdb" height="30rem">
select station, avg(temp) from readings group by 1
</sv-table>
```

The tag is the extension's `name`. Attributes and body mean whatever the
extension decides — sideview does not interpret them, it delivers them (see
`SIDEVIEW_BLOCK`). The body is raw bytes to the closing tag, like every
sideview block: authors never escape anything.

One attribute is reserved: `height`, a CSS length. When present the frame is
pinned to it; when absent sideview measures the frame's document and grows
the block to fit.

## The three render modes

**`frame`** — the normal mode for an app-like extension. `entry` is served
into an iframe on **sideview's own origin** (no port, no CORS, works over a
tailnet and through one SSH forward). The frame's base URL is **opaque** —
never construct absolute paths; sideview injects `<base href>` so relative
references (`./app.js`, `fetch("data")`) resolve to your directory. Your CSS
and JS are your own: nothing of the page leaks in, nothing of yours leaks
out.

**`inline`** — the extension's entry is rendered *into* the page: it inherits
Bootstrap 5, the design tokens and the theme, and its text is commentable
exactly like prose. The price: its CSS is global in both directions. Use it
for extensions that want to look like part of the document rather than an
app in it. Relative asset references are rewritten to the extension's
directory at render time.

**`sandbox`** — reserved for embedding a foreign app that should get no
access and offers none. Not built; declare it and you get the honest
degradation block.

Choose the mode yourself — it is a declaration about what your CSS and JS
are written against, which only you know. The installer never overrides it.

## What sideview injects into a frame

Before your entry's first script runs:

- `<base href="…">` — the opaque per-block base. Rely on it for your own
  assets; never construct your own absolute paths from `location.origin`.
  The one sanctioned exception: sideview's public endpoints are same-origin
  and fair game — `fetch("/f/<project-relative path>")` reads a project file
  without spawning anything.
- `window.SIDEVIEW_BLOCK` — `{ page, id, attrs, body }`: the block's page id,
  block id, attributes as an object of strings, and the raw body. This is
  how your UI learns what it is showing without asking anyone.
- `window.sideview` — the API below.
- A live `data-bs-theme="light" | "dark"` attribute on your `<html>`,
  kept in sync with the page's theme. Key your CSS off it and the block
  follows the reader's toggle.

## The API: two functions

```js
const { code, stdout, stderr } =
  await sideview.call_cli(["sql", "data.duckdb", "select 1", "--format", "jsonl"],
                          { stdin: "" });

const response = await sideview.call_cli_streaming(["export", "--format", "csv"]);
for await (const chunk of response.body) { /* paint as it arrives */ }
```

- **What runs**: always and only the manifest's `bin`. The plugin chooses
  arguments, never the executable. Arguments are passed as an argv array —
  there is no shell, no quoting, no interpolation.
- `call_cli(args, {stdin})` → `Promise<{code, stdout, stderr}>`. `stdout`
  and `stderr` are UTF-8 text — a binary that emits bytes (an image, a
  parquet) must be called through the streaming variant, whose `Response`
  carries them faithfully. Large inputs belong on `stdin`, not in argv.
- `call_cli_streaming(args, {stdin})` → a `fetch` `Response` whose body
  streams the child's stdout as it is produced. **Aborting the request
  (AbortController) kills the child** — wire it to scroll-away or re-run
  and cancellation costs nothing.
- The child runs with **cwd at the project root**; paths in `attrs`/`args`
  are project-relative.
- **Caps** (mechanism-enforced, not yours to opt out of): a per-call timeout
  (default 30s), an output cap (default 64 MB), and the child dies with the
  daemon. A non-zero exit from `call_cli` still resolves (read `code`); a
  child that dies mid-`streaming` after the first byte can only truncate —
  the error goes to the daemon's log, so emit your own end-marker if your
  format needs one.

There is no process-shaped third function — no resident child, no sessions
held open: see "The contract" below for why, and the ladder for what to do
if you believe you need one.

## Comments from inside an extension

Specified now, lands after the first build. Sideview places a block-level
comment bubble on every extension block for free; these three give a frame
*finer* anchors — a cell, a row — that only it can address:

- `sideview.create_comment({anchor, quote, context})` — opens a pre-anchored
  draft in sideview's comment bar; the human writes and sends there, where
  attachments and markdown already live. `anchor` is yours (stored as
  `c:<your string>`, opaque to sideview); **`quote` must carry the meaning
  without your UI visible** ("`revenue`, row 1,204: −£3,412") — it is what
  the agent's watch event shows and what survives when the anchor stops
  resolving. This never posts a comment by itself.
- `sideview.get_comments()` → the block's threads and comments, and
  `sideview.on_comments(cb)` — called with the same shape on every change,
  so highlighting commented cells stays correct without polling.
- **Jump delegation, inward**: when a reader clicks a `c:` thread's
  jump-back, sideview cannot resolve your anchor, so the frame receives a
  `sideview:jump` event with it — scroll your own UI, flash your own cell.

Orphaning is yours: sideview never marks a `c:` thread "§ changed", because
it cannot know whether "row 1,204 of this query" still exists. If you can
detect staleness, say so in your UI; the captured quote is what keeps the
thread meaningful either way.

## The contract: every call is a fresh process

Your binary must assume **a fresh process per invocation** — files on disk
are the only memory between calls. This is load-bearing, not stylistic: the
mechanism spawns per call, and any feature that quietly assumes server
memory (a prepared temp table, a cached connection) will misbehave in ways
no one will attribute correctly. Measured while designing this: a compiled
binary opening a million-row DuckDB file and answering a query costs ~30ms
end to end, which is beneath notice next to a human clicking Run —
concurrency here is a handful of viewers, not a public site, so
process-per-request drains nothing that matters.

If two calls can write the same file (an autosave racing a run), your binary
owns that locking; sideview serializes nothing.

### The ladder — argue your way out, never in

static files → `call_cli` per request → resident process.

Take a step only on a demonstrated failure of the previous one. What
qualifies for the last step: a held connection (a PTY, websockets), state
genuinely too expensive to rebuild per call (an LSP-sized index — a DuckDB
file open at 30ms is the counterexample), or push-not-pull. **Streaming does
not qualify** — `call_cli_streaming` streams. The resident-process rung is
not yet specified; if your extension truly needs it, that is a design
conversation, not a workaround.

## Sizing, theme, comments

- **Sizing**: pinned when the block says `height`; otherwise sideview
  measures your document and follows it. Grids and other viewport-filling
  UIs should expect to be pinned.
- **Theme**: mirror `data-bs-theme` (injected, live) into your styles.
- **Comments**: readers comment on the block as a whole — a bubble sideview
  places on blocks that offer no selectable text. Finer-grained commenting
  from inside an extension (a cell, a row) is a reserved capability: a
  frame will be able to *offer* an anchor by posting
  `{sv: 1, type: "comment", anchor, quote, context}` to its parent. Not yet
  built; design your quote text to be meaningful without your UI visible.

## What content cannot do

Agent-authored `sv-html` blocks are *content*, not extensions: they render
in opaque-origin sandboxed iframes, and the extension endpoints send no CORS
headers, so content cannot invoke `call_cli` or read your frame. Nothing a
page author writes can borrow an installed extension's authority.

## Checklist: adapting an existing local web app

The common case — you have an app that serves its own UI and answers its own
HTTP — and it maps to this mechanism as follows:

1. **Ship your UI as static files** in the extension directory; drop the
   server. Build with relative asset paths (`base: './'` in Vite) so the
   opaque base works.
2. **Replace every `fetch` to your own server** with `sideview.call_cli`
   against your CLI's existing verbs. If your CLI can already answer the
   question (`yourtool query --format jsonl`), the adapter is argument
   plumbing, not new code.
3. **Delete held connections** (SSE, websockets). Embedded, coordination is
   sideview's job; your UI renders what it is given and re-asks on demand.
4. **Make one-shot verbs cover your inputs.** If your CLI can only answer
   over one file format, teach it the others — the one-shot path *is* the
   interface now.
5. **Take initial state from `SIDEVIEW_BLOCK`**, not from routes or
   query strings: the block's attrs and body are your configuration.
6. Read the contract section again: no memory between calls; files are
   state; you own your locking.

## Worked example: the smallest real extension

`extensions/wordcount/plugin.toml`

```toml
name = "wordcount"
api = 1
render = "frame"
entry = "index.html"
bin = "wc"
```

`extensions/wordcount/index.html`

```html
<!doctype html>
<meta charset="utf-8">
<body>
  <pre id="out">counting…</pre>
  <script type="module">
    const { attrs, body } = window.SIDEVIEW_BLOCK;
    const { code, stdout, stderr } =
      await sideview.call_cli([attrs.mode === "lines" ? "-l" : "-w"],
                              { stdin: body });
    document.getElementById("out").textContent =
      code === 0 ? stdout.trim() + " " + (attrs.mode || "words") : stderr;
  </script>
</body>
```

A page uses it as:

```
<sv-wordcount mode="lines">
any text at all,
counted by a subprocess.
</sv-wordcount>
```

That is the entire mechanism: a manifest, files, a tag, a block, two
functions.

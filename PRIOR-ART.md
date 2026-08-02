# Prior art

Surveyed before starting work. The headline: **no one is building this exact thing, but
every component of it has strong, mature prior art** — and two of those projects are close
enough that they deserve to be tried before we write any code.

## The visual side channel

Taking the reframed problem first — CLI agents have no good way to *show* you things, and
everything they do show costs context — because it turns up the most directly competitive
prior art.

### The out-of-band principle is established, and named

MCP already has the mechanism: a tool can return **`ResourceLink`** — a URI plus metadata —
instead of content, with the actual data read separately, bypassing the context window
entirely. The recommended pattern is explicitly dual-response: a compact summary for the
model, `structuredContent` for the rich client-side widget at **zero token cost**, and a
plain HTTP URL for the full dataset served outside MCP. There's [an arXiv
paper](https://arxiv.org/html/2510.05968v1) on extending this for large datasets.

This is strong validation: the core idea is not speculative, it's a recognised pattern with
a spec-level affordance. Sideview's version is more radical only in that the *renderer* is
a page the daemon owns rather than a widget in a chat client.

### Companion web UIs for CLI agents already exist — and miss the point

**The Companion** (Claude Code), **Pi Web**, **aider's browser mode** and **Aider WebUI**
all put a CLI agent's session in a browser: streaming responses, collapsible tool blocks,
syntax highlighting, multiple concurrent sessions, session persistence.

They are **transcript mirrors**. They render the conversation more nicely — but everything
they display already went through the model's context to get there. A nicer view of an
expensive channel is not a second channel. None of them gives the agent a way to display
something the model never had to carry, which is the entire point of the reframe.

### Wave Terminal is the closest thing that exists, and you should try it

**Wave** is a terminal whose workspace is made of blocks — terminal, file preview, web
browser, AI chat — with inline graphics, where opening a CSV gives you a real table, a PNG
gives you the PNG, a PDF gives you the PDF. Critically, its **`wsh`** command line
*controls those graphical blocks from the shell*, passing data between terminal sessions and
GUI components.

That is a large fraction of the reframed idea, shipped: a CLI that makes graphical things
appear, with no context cost, driven from the same shell the agent is already using.

Its remote and persistence story is also solid, which is worth knowing before dismissing it.
Wave has SSH connections, and **durable sessions**: a lightweight job manager on the remote
host keeps the shell process running independently of the Wave connection, so state, running
programs and history survive a dropped network, sleep, or closing Wave entirely — tmux's
guarantee, built in and without installing anything on the remote.

**But the display surface is bound to the Wave client, and that is disqualifying.** The whole
reframe is that an agent should be able to show you something regardless of where or how it's
running. Wave's blocks render in Wave, so the answer to "I'm in herdr on a remote box over
Tailscale" is *replace that setup with Wave*. Its durable sessions don't compose with an
existing multiplexer either — they compete with it, and there's [an open issue about SSH
connections failing when tmux is in play](https://github.com/wavetermdev/waveterm/issues/1772).
Augmenting your workflow and replacing your terminal are different offers.

It also isn't a **document**: blocks are panes in a workspace, not an ordered narrative with
prose, and there's no plan model, no provenance, no snapshot or sharing story, and no
supervision of a project's own services inside a document.

**Worth an afternoon, not a week** — enough to feel what CLI-driven graphical blocks are
like, since `wsh` is the best existing expression of that idea. Not a candidate replacement,
because terminal lock-in is exactly the constraint sideview exists to avoid.

### The pattern behind both near-misses

Wave binds the display to a **client app**; the kitty graphics protocol binds it to a **live
tty**; MCP-UI binds it to a **supporting chat host**. Each integrates the visual surface into
something, and thereby inherits that thing's constraints.

Sideview's position is that the surface should be bound to **nothing** — a URL served by a
daemon, which is why it works identically in kitty, herdr, tmux, a bare SSH session, VS
Code's terminal, or a cron job with no tty at all. That last case is the clean test: a
scheduled agent at 3am can append views to a plan you read at 9am. Terminal graphics can't
(no tty), Wave can't (no client attached), MCP-UI can't (no conversation). See
[DESIGN.md](DESIGN.md) for how remote and persistence fall out of that.

### Terminal graphics solve part of it today

The **kitty graphics protocol** transmits true 32-bit RGBA images with async rendering and
compositing, and is spreading to WezTerm, Ghostty and Konsole; Sixel is the older
alternative. Since kitty is already in use here, **"I want to see a screenshot" has a
solution available this afternoon**, at zero context cost and zero build cost.

Worth setting up before treating image display as a reason to build software. What terminal
graphics can't give you is layout, interaction, a persistent surface, or anything with a
process behind it.

### What's actually left

After Wave and terminal graphics take their share, the durable gap is narrower but real:
**an authored, persistent, prose-bearing document whose live parts bypass the model's
context, with provenance and a feedback channel back to the agent.** Wave has the side
channel without the document; the companion UIs have the session view without the side
channel; MCP-UI has the components but routes them through the conversation and needs a
supporting host.

## By axis

### A document with computation in any language

**Observable Framework** is the closest architectural relative. Its *data loaders* are
programs in any language whose stdout becomes a page's data — Python, R, shell, anything —
and its preview server watches them: touch a loader and it re-runs, pushes new data down,
and re-evaluates referencing code with no reload. That is precisely sideview's
subprocess-and-patch model. The difference is direction: Framework runs loaders at *build*
time and emits a static site, which is why it can be hosted anywhere and why it explicitly
tells you to use remote endpoints if you need genuinely live data.

**Quarto** contributes a concept we should adopt by name: *execution contexts*, the
distinction between code that runs at render time and code that runs while serving. It also
states our central tradeoff plainly — `server: shiny` gives real interactivity but requires
a live runtime per viewer, while htmlwidgets keep everything client-side and deploy
anywhere. Same fork, same reasons.

**marimo** is a reactive Python notebook stored as pure Python, git-friendly, deployable as
an app, and exportable to self-contained WASM HTML that runs with no server. **Livebook**
(Elixir) is the same genre with a better extension story, below. **Jupyter** is the
substrate everyone else reacts to.

### An extensible registry of typed blocks

**Livebook smart cells** are the strongest validation of the three-tier block model.
High-level task cells — query a database, plot a chart, build a map — driven by UI rather
than code, with built-ins for Postgres and MySQL and a documented path
(`Kino.SmartCell.register/1`) for anyone to write and publish their own. That is exactly
tier 2 and tier 3, shipped and in use, and the registration contract is worth reading
closely before designing ours.

**Datasette** is the other model: a small core plus a plugin hook ecosystem, widely
described as its superpower.

### Agents rendering UI

**MCP Apps** became the first official MCP extension (announced Nov 2025, live Jan 2026),
letting tools return interactive UI that renders in the conversation. Its security model is
mandatory sandboxed iframes with auditable `postMessage`/JSON-RPC communication — the same
isolation choice sideview made, arrived at independently, which is reassuring.

The contrast matters though: MCP Apps' container is a *chat turn* and its sandbox exists
specifically to deny local process access. Sideview's container is a *document* and local
process access is the entire reason it exists. They are not competitors; if anything an MCP
Apps surface is a plausible later front-end for a sideview block.

**Claude Artifacts** is the limitation the README opens with, in shipped form: rich,
interactive, sandboxed — and therefore unable to touch the real database or toolchain.

### SQLite as the artifact

**Datasette** is the reference implementation of the idea we should copy hardest for
sharing: it treats SQLite files as read-only and immutable, cannot execute INSERT or UPDATE
at all, and therefore can expose arbitrary `SELECT` to the public without worrying about
injection. Safety by construction rather than by validation. `datasette publish` is also the
shape `sideview publish` should take.

### Sharing something live and local

- **Voila** turns a notebook into a dashboard that strips input cells and — the important
  part — **disallows execute requests from the front end entirely**. Interactivity comes
  only from pre-configured widgets. This is a proven, auditable, one-sentence rule, and it
  is exactly the switch sideview's read-only share mode needs.
- **Tailscale Serve** solves reachability with identity attached: route a localhost service
  to your tailnet, and Tailscale sets `Tailscale-User-Login` to the authenticated caller's
  email, with app capabilities forwarded as headers. Their own best practice is to keep the
  service listening on localhost only, so header spoofing isn't possible — which is the bind
  we already chose. David is already on a tailnet, so this is close to free.
- **sshx** shares a live terminal by link, end-to-end encrypted (Argon2/AES) through a
  relay mesh the operator can't read, with a Rust server. The transferable pattern is
  relay-plus-client-held-key: confidentiality without trusting the middle.
- **Binder / JupyterLite / marimo WASM** take the opposite route — ship the runtime to the
  viewer's browser so nothing executes on the author's machine. Radical, and worth keeping
  in mind as the reason to keep block outputs serializable.

## Not writing the artifact from scratch

There is substantial prior art, and the most instructive case is the one that already
disappoints: **Claude Artifacts ship Tailwind, shadcn/ui, lucide icons and Recharts
preloaded**, so a model there is already meant to assemble rather than author from nothing.

The reason it still feels slow is worth being precise about, because it's the whole
argument for rung 2: **that library is at the wrong altitude.** shadcn is a general-purpose
construction kit of unstyled primitives, and Tailwind describes appearance element by
element, so the model still composes every layout and re-specifies every visual decision in
utility classes. A callout ends up as

```html
<div class="rounded-lg border border-amber-200 bg-amber-50 p-4 flex gap-3 items-start
            dark:border-amber-900 dark:bg-amber-950/40">
```

where a domain vocabulary makes it `<div class="sv-callout sv-risk">`. Same output, an order
of magnitude fewer tokens, and no opportunity to be subtly inconsistent with the block above
it. General kit versus domain vocabulary is the actual lever, and nobody has built the domain
vocabulary for *plans*.

The rest of the field:

**Adaptive Cards** is the canonical declarative-UI-rendered-by-host specification: purely
declarative JSON with no code, automatically styled to match the host application's own UX
and brand, and — the part worth copying — a **versioned schema with mandatory fallback**,
where a renderer encountering a card newer than it supports must render `fallbackText`
rather than fail. Slack's Block Kit is the same genre.

**MCP-UI's remote-dom mode** is the closest analogue to rung 2 in the agent world: the server
sends a UI *description*, it executes sandboxed in a Web Worker inside an iframe, and it
renders through the **host's own component library**, so the result matches the host's look
and feel. Their documented tradeoff is the same one we're making — better than iframes for
integration and performance, at the cost of client infrastructure and component-library
management.

**A2UI** (Google, open project for agent-driven interfaces) is being combined with MCP Apps,
which suggests declarative agent UI is consolidating into a standard rather than staying a
per-vendor trick.

**Vercel's AI SDK generative UI** has models tool-call into predefined React components,
streamed server to client via RSC. Useful as a shape, but note the warning: **RSC development
is paused and Vercel now flags it experimental**, steering production users to plain
`useChat`. Don't copy that architecture on the assumption it's settled.

**Streamlit, marimo and Livebook** are the empirical support for the size limit. LLMs write
Streamlit unusually well, and the reason is that its vocabulary is small, flat and guessable
— which is the argument for holding sideview's class list to something an agent can keep in
its head.

### What this changes

The concept is well-precedented; the gap is the vocabulary's subject. Every existing set is
either general-purpose UI (shadcn, Tailwind), chat-message-shaped (Adaptive Cards, Block
Kit), or app-shaped (Streamlit). None is document-shaped, and none has the pieces a design
argument is actually made of — risks, assumptions, options, before/after, decision matrices,
measured figures.

Two concrete borrowings:

- **Version the block schema with a fallback rendering, from day one.** Adaptive Cards has
  this because hosts and cards drift apart, and sideview has the same problem twice over: a
  snapshot opened a year later, and a plugin block meeting an older daemon. A block spec
  carrying a version and a fallback degrades instead of breaking. It's cheap now and
  expensive to retrofit — the same shape of decision as typed params.
- **Host-rendered beats iframe-rendered for anything meant to look like part of the page**,
  which is MCP-UI's finding and confirms rung 2 rather than rung 1 should be the default
  path.

## What to steal, concretely

1. **Livebook's smart-cell contract** as the model for the block registry — read it before
   designing tier 2/3.
2. **Observable's data-loader semantics**: subprocess in any language, stdout is the value,
   cache keyed on inputs, invalidate on mtime. Validates the sidecar-file design.
3. **Voila's rule** — no execute requests from the front end — as the literal implementation
   of read-only sharing.
4. **Datasette's immutability**: viewer-reachable queries run against read-only handles with
   bound parameters, never interpolated SQL. Injection stops being a category.
5. **Tailscale Serve** for reachability *and* identity, which also gives comments a real
   author instead of an anonymous token holder.
6. **Quarto's execution contexts** as explicit vocabulary in the block spec: does this block
   run at author time or at view time?

## The gap

Nothing found targets **the plan itself** as the artifact. Notebooks argue for findings,
Streamlit and Gradio build tools, Framework and Quarto publish reports. Sideview argues for
a *decision* — a document whose job is to make a proposed change believable, with evidence
blocks attached to claims and a reader whose response flows back to the author.

Three things follow from that and appear to be genuinely unoccupied:

- **The author is an agent**, so the interface is designed for one: a CLI over a local
  SQLite file, chosen because agent sandboxes block localhost daemons. Every tool surveyed
  assumes a human at a keyboard, so none of them made this choice.
- **Prose is the primary content and code is the exception.** Every notebook has it the
  other way round.
- **The reader's reaction is part of the artifact** — comments and choices land back in the
  store for the agent to act on. Notebooks have no return channel to their author.

## Why the notebooks don't reach it: lifecycle, not language

The obvious "just use marimo (or Livebook)" objection fails, and for a more interesting
reason than Python-centricity. Marimo *can* shell out to anything and render what comes
back, so any language is technically reachable.

The real limit is that **every tool surveyed models computation as `evaluate → value`**: run
a program, capture its output, cache it, render it. Notebook cells do it, and so do
Observable's data loaders — a loader runs, its stdout is captured, and that's the end of its
life.

What sideview is for is `supervise → endpoint`: start a section of the real project, keep it
alive across the reader's interactions, give the page a live channel to it, tear it down
afterwards. No cell model has a notion of owning a process with a lifetime, and reactive
re-execution actively fights one. You can point an iframe at a dev server you started
yourself in another terminal, but nothing supervises it, nothing allocates its port, nothing
kills it — that's a trick, not a feature.

That gap is where the whole project lives, and nothing found occupies it.

## The baseline still worth testing, scoped down

An agent authoring a marimo notebook remains the cheapest route to the **value-block** half
— tables, diffs, charts, query plans — with reactive execution, git-friendly storage and a
share story for free. It cannot do service blocks at all.

So the one-day experiment is still worth running, but it tests something narrower than I
first suggested: not "is sideview necessary" — service blocks answer that — but "**how badly
does a code-cell-shaped, prose-second document read as a plan?**" If an agent-authored
marimo notebook turns out to be a perfectly good *argument*, then sideview's document model
can stay thin and the effort belongs almost entirely in the service-block machinery. If it
reads as a program with commentary, the document model is load-bearing too. Either answer
usefully redirects the work.

## Sources

- [Observable Framework](https://github.com/observablehq/framework) · [data loaders](https://observablehq.com/framework/data-loaders) · [live data discussion](https://github.com/observablehq/framework/discussions/876)
- [Quarto interactivity](https://quarto.org/docs/interactive/) · [Shiny documents](https://quarto.org/docs/interactive/shiny/)
- [marimo](https://github.com/marimo-team/marimo) · [WASM export](https://docs.marimo.io/guides/exporting/webassembly_html/)
- [Livebook](https://github.com/livebook-dev/livebook) · [smart cells v0.6](https://news.livebook.dev/v0.6-automate-and-learn-with-smart-cells-mxJJe) · [Kino.SmartCell](https://hexdocs.pm/kino/Kino.SmartCell.html)
- [MCP Apps announcement](https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/) · [SEP-1865](https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp) · [mcp-ui](https://mcpui.dev/guide/embeddable-ui)
- [Datasette](https://datasette.io/) · [plugins](https://docs.datasette.io/en/stable/ecosystem.html)
- [Voila](https://github.com/voila-dashboards/voila) · [customizing](https://voila.readthedocs.io/en/stable/customize.html)
- [Adaptive Cards](https://learn.microsoft.com/en-us/adaptive-cards/) · [schema explorer](https://adaptivecards.io/explorer/) · [implementing a renderer](https://learn.microsoft.com/en-us/adaptive-cards/rendering-cards/implement-a-renderer)
- [MCP-UI remote-dom renderer](https://mcpui.dev/guide/client/remote-dom-resource.html) · [technical overview](https://workos.com/blog/mcp-ui-a-technical-deep-dive-into-interactive-agent-interfaces)
- [A2UI](https://developers.googleblog.com/introducing-a2ui-an-open-project-for-agent-driven-interfaces/) · [A2UI + MCP Apps](https://developers.googleblog.com/a2ui-and-mcp-apps/)
- [AI SDK 3.0 generative UI](https://vercel.com/blog/ai-sdk-3-generative-ui)
- [Claude Artifact Runner](https://github.com/claudio-silva/claude-artifact-runner) · [reverse-engineering artifacts](https://www.reidbarber.com/blog/reverse-engineering-claude-artifacts)
- [Tailscale Serve](https://tailscale.com/kb/1242/tailscale-serve) · [app capabilities in headers](https://tailscale.com/blog/app-capabilities)
- [sshx](https://sshx.io/) · [repo](https://github.com/ekzhang/sshx)

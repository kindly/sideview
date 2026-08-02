# sideview

A visual side channel for CLI agents. Nothing is built yet — this repo is currently five design
documents and a licence.

**Read [V0.md](V0.md) first.** It is the scope actually being built. [README.md](README.md) is the
premise, [DESIGN.md](DESIGN.md) is the long-term architecture and backlog,
[SHARING.md](SHARING.md) and [PRIOR-ART.md](PRIOR-ART.md) are supporting research.
[HANDOFF.md](HANDOFF.md) has current state and what to do next.

## Things not to undo

These were each argued out at length. Changing them is fine, but do it deliberately, not by
drift:

- **Scope is v0 only** — prose, markup, html and image blocks. Service blocks, tables, diffs,
  provenance and sharing are all explicitly deferred. The project has been re-scoped twice; do not
  re-expand it casually.
- **Reference, never embed.** A block spec holds a path, a query or a command — never the content.
  The entire point is that data reaches the page without passing through the model's context. If
  an agent must read the data in order to display it, the block is designed wrong.
- **Nothing under `$HOME`.** All state lives in `.sideview/` in the project, because the agent
  sandbox permits writes to the working directory and not the home directory.
- **Borrow the design vocabulary, don't invent one.** Pico.css for semantic HTML, Bootstrap-
  compatible names (`alert alert-warning`, `card`, `badge`) where HTML has no element, and only a
  handful of `sv-` classes for what has no precedent.
- **Rust, actix-web, rusqlite, SSE.** Not websockets — see the reasoning in V0.md before changing
  it.

## The sandbox constraint, which explains most of the design

Each sandboxed Bash invocation gets its own network namespace. `bind()` succeeds inside it but the
port does not exist on the host, so **an agent cannot start a daemon the browser can reach**. This
is why the daemon is started by hand, why every agent→daemon channel is the SQLite store, and why
liveness is a timestamp rather than a ping.

## Working here

Docs are the deliverable for now. Keep them in the same register as they are: decisions with their
reasons attached, rejected alternatives recorded so they aren't re-proposed, and honest notes about
what is uncertain.

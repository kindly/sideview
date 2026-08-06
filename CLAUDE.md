# sideview

A visual side channel for CLI agents. v1 shipped as **0.1.0 on crates.io** (2026-08-07,
tag `v0.1.0`, github.com/kindly/sideview); the design documents remain the authority on
intent.

**Read [V2.sv](V2.sv) first** — the working plan, and itself the first committed sideview
document page (bind it to a running daemon to view it live). The rationale behind every v2
feature is in [V1.md](V1.md)'s committed-to-v2 entries; V1.md and [V0.md](V0.md) specify what
already shipped and are not re-opened. [README.md](README.md) is the
premise, [DESIGN.md](DESIGN.md) is the long-term architecture and backlog,
[SHARING.md](SHARING.md) and [PRIOR-ART.md](PRIOR-ART.md) are supporting research.
[HANDOFF.md](HANDOFF.md) has current state and what to do next.

## Things not to undo

These were each argued out at length. Changing them is fine, but do it deliberately, not by
drift:

- **Scope is v2 — closing the feedback loop** (V2.sv): comments from the page (db-stored,
  Sphinx-style hover placement), `sideview watch` as the agent's blocking await, watched
  diffs via gitoxide, explicit agent outlines, document-page registration. Two earned
  placement principles govern everything: *sv files are version-control-worthy canon; the db
  holds what should not be versioned* — and *the page file has one author; everything
  multi-writer goes through SQLite*. Tables, app subprocesses, service blocks, provenance and
  sharing remain explicitly deferred. The project has been re-scoped twice; do not re-expand
  it casually.
- **Reference, never embed.** A block spec holds a path, a query or a command — never the content.
  The entire point is that data reaches the page without passing through the model's context. If
  an agent must read the data in order to display it, the block is designed wrong.
- **Nothing under `$HOME`.** All state lives in `.sideview/` in the project, because the agent
  sandbox permits writes to the working directory and not the home directory.
- **Ship the framework the model already knows: real Bootstrap 5, CSS only.** Plus a prose layer
  for bare markdown elements, a v4-compat shim, and a handful of `sv-` classes for what has no
  precedent. This replaced "Pico + a borrowed subset of Bootstrap names" on 2026-08-03 — the
  subset silently no-opped the layout/utility classes models actually emit. Reasoning and the
  Tailwind/daisyUI rejection are in V0.md's design-system section; don't relitigate without
  reading it.
- **Rust, actix-web, rusqlite, SSE.** Not websockets — see the reasoning in V0.md before changing
  it.

## The sandbox constraint, which explains most of the design

Each sandboxed Bash invocation gets its own network namespace. `bind()` succeeds inside it but the
port does not exist on the host, so **an agent cannot start a daemon the browser can reach**. This
is why the daemon is started by hand, why the agent→daemon channels are writes the sandbox allows —
page files in the project, plus the SQLite store for bindings and liveness — and why liveness is a
timestamp rather than a ping.

## Working here

Dogfood first: nothing is implemented before its design has appeared on the live sideview page
(author's rule, set at v1 scoping). Keep that page terse — features and goals; rationale
belongs in the design docs.

Code and docs move together: when the code diverges from a documented decision, update the doc in
the same change, in the same register. Keep docs as they are: decisions with their
reasons attached, rejected alternatives recorded so they aren't re-proposed, and honest notes about
what is uncertain.

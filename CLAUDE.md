# sideview

A visual side channel for CLI agents. The latest release on crates.io is **0.5.0**
(2026-08-26, github.com/kindly/sideview); the design documents remain the authority on
intent.

**Read the working plan first** — the highest-numbered `V*.sv` in the repo, itself a
sideview document page (bind it to a running daemon to view it live); its rationale lives
on the grill page that curated it and in the design docs. The lower-numbered plans (`V*.sv`
and `V*.md`) specify what already shipped and are not re-opened.
[README.md](README.md) is the premise, [DESIGN.md](DESIGN.md) is the long-term architecture
and backlog, [VISION.sv](VISION.sv) is the north star, [IDEAS.sv](IDEAS.sv) is the
continuing pool versions are curated from, [SHARING.md](SHARING.md) and
[PRIOR-ART.md](PRIOR-ART.md) are supporting research. [HANDOFF.md](HANDOFF.md) has current
state and what to do next.

## Things not to undo

These were each argued out at length. Changing them is fine, but do it deliberately, not by
drift:

- **Scope is whatever the working plan commits to, and nothing more.** The current plan is
  structural — fewer concepts, same features, measured as *context needed per change* — and
  its "deliberately not touched" list records the keeps: no feature is removed. Two earned
  placement principles govern everything (argued out in the shipped plans): *sv files are
  version-control-worthy canon; the db holds what should not be versioned* — and *the page
  file has one author; everything multi-writer goes through SQLite*. Tables, app
  subprocesses, service blocks, provenance and sharing remain explicitly deferred. Versions
  are curated deliberately from the pool; do not re-expand scope casually.
- **Reference, never embed.** A block spec holds a path, a query or a command — never the content.
  The entire point is that data reaches the page without passing through the model's context. If
  an agent must read the data in order to display it, the block is designed wrong.
- **Nothing under `$HOME`.** All state lives in `.sideview/` in the project, because the agent
  sandbox permits writes to the working directory and not the home directory.
- **Ship the framework the model already knows: real Bootstrap 5, CSS only.** Plus a prose layer
  for bare markdown elements, a Bootstrap-4-compat shim, and a handful of `sv-` classes for what
  has no precedent. This replaced "Pico + a borrowed subset of Bootstrap names" on 2026-08-03 —
  the subset silently no-opped the layout/utility classes models actually emit. Reasoning and the
  Tailwind/daisyUI rejection are in [V0.md](V0.md)'s design-system section; don't relitigate
  without reading it.
- **Rust, actix-web, rusqlite, SSE.** Not websockets — see the reasoning in [V0.md](V0.md)
  before changing it.

## The sandbox constraint, which explains most of the design

Each sandboxed Bash invocation gets its own network namespace. `bind()` succeeds inside it but the
port does not exist on the host, so **an agent cannot start a daemon the browser can reach**. This
is why the daemon is started by hand, why the agent→daemon channels are writes the sandbox allows —
page files in the project, plus the SQLite store for bindings and liveness — and why liveness is a
timestamp rather than a ping.

## Working here

Dogfood first: nothing is implemented before its design has appeared on the live sideview page
(author's rule, set at the first scoping). Keep that page terse — features and goals; rationale
belongs in the design docs.

Code and docs move together: when the code diverges from a documented decision, update the doc in
the same change, in the same register. Keep docs as they are: decisions with their
reasons attached, rejected alternatives recorded so they aren't re-proposed, and honest notes about
what is uncertain.

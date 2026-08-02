# Sharing

The design so far gives essentially no sharing: localhost bind, per-plan token, a daemon
that dies with the agent. That's the right default and the wrong ceiling — a plan that
can't be shown to a colleague is a plan that can't do its job.

The problem looked impossible because two separate things were tangled together.

**Reachability** — can the viewer's browser get to the page? Network question. Solved,
cheaply, by Tailscale.

**Authority** — whose credentials do the blocks run with? Security question. The hard one,
and unrelated to the first.

Keeping them apart makes most of this tractable, because three of the four useful sharing
modes below need no execution on the author's machine at all.

## The ladder

### T0 — Share the snapshot *(ships with v1)*

Frozen HTML: prose intact, every live block rendered as its last computed output. No server,
no token, no execution, no expiry. Mail it, commit it, drop it in a ticket.

This covers most of what "share the plan" actually means — let someone read the reasoning
and see the evidence. It carries zero execution risk and needs nothing built beyond the
snapshotter that already exists for expiry.

**The real risk here is data, not code, and it's easy to miss.** A snapshot bakes in
whatever the blocks computed: query results, file contents, possibly a connection string in
a displayed command line. Sending it is a data disclosure. So `sideview snapshot` should
report what it is embedding — which blocks, which sources, how many rows from where — and
support marking a block as excluded from snapshots. A quiet snapshot command is a
credential leak waiting to happen.

**Provenance sharpens this considerably, in both directions.** It makes a shared snapshot
much more valuable — the reader sees the commit, the tree state and the input hashes behind
every number, and can grade how much to trust it. It also makes the disclosure much larger,
because provenance includes **the full uncommitted working diff**. Mailing someone a plan
would then mail them your in-progress source, which is not what anyone thinks they are doing
when they share a document.

So: the diff is recorded locally but **excluded from snapshots and packs by default**,
reduced to commit SHA plus a dirty flag plus the diff's hash — enough to prove two artifacts
came from identical state, without shipping the code. `--include-diff` opts in, and the
disclosure report names it explicitly.

### T1 — Pack and ship the store *(cheap, and underrated)*

Because the plan is a SQLite file, sharing can mean handing over the plan itself:
`sideview pack` bundles the db plus the data files its blocks reference, and the recipient
runs their own daemon against it.

The security properties are excellent and come free: blocks execute with the *recipient's*
authority on the *recipient's* machine. There is no cross-machine execution to reason about,
nothing exposed on a network, and the recipient can poke at everything — re-run, edit params,
change the query — because it's theirs now.

It works when a block's sources are portable (a parquet file, a CSV, a scratch DB you can
bundle) and fails when a block needs your production database or your local toolchain. For
GEM-shaped work — "here is a query over a parquet file" — portable is the common case, which
makes this a better primary sharing story than it first appears.

**With provenance, T1 becomes verification rather than just transfer.** The pack carries the
input hashes and each output's hash, so the recipient's daemon checks the bundle matches what
the author measured, re-runs, and compares. A `verifiable`-grade plan that reproduces on a
second machine is about as strong as a decision document gets — and the block grades tell the
recipient upfront which parts can be checked that way and which can only be taken on
authority.

### T2 — Live, read-only, over the tailnet

For "watch me build this" and "look at it while it's still running".

**Reachability:** `tailscale serve` in front of the localhost-bound daemon. Tailscale sets
`Tailscale-User-Login` to the caller's real email and forwards app capabilities as headers,
and their own guidance is to keep the service on localhost so those headers can't be spoofed
— which is the bind we already chose. Identity comes from the tailnet, so tailnet viewers
need no token at all; tokens stay only for the tunnel case.

Real identity also fixes comments: a shared plan's feedback rows get an actual author rather
than "whoever had the link".

**Authority:** the viewer's session is read-only, enforced server-side by role rather than
by hiding buttons — Voila's rule, literally: no execute requests accepted from the front end.
Blocks display; nothing re-runs.

**Agent-authored blocks share live, scripts intact.** An earlier draft restricted shared
plans to built-in blocks, on the confused-deputy argument that arbitrary JavaScript holding a
valid session to a process-running API is dangerous. That imported a public-web threat model
into a named-colleague one, and it would gut the most useful block type precisely when you
want to show someone something.

The argument doesn't survive contact: block script runs in the plan's own origin and can't
reach the viewer's other sessions; the authority it would borrow is already removed by
enforcing read-only on the session's role server-side rather than by inspecting content; and
the realistic baseline is the org's own code review, through which colleagues already ship
each other arbitrary JavaScript every day. A stricter bar here than on the code path everyone
already trusts is theatre.

**Third-party plugin blocks stay a separate question** — agent-authored markup in your own
plan is code you own, a plugin is code you didn't write and didn't review. And a
`connect-src 'self'` CSP on shared pages closes the accidental-exfiltration path essentially
for free, since legitimate blocks fetch from the daemon rather than third-party hosts. Useful
hygiene; not the boundary that matters.

### T3 — Constrained interactive *(the ambitious end; not v1)*

The viewer can re-run and vary things. Defensible only if what can run is bounded *by
construction*, not by trust — and there is good precedent for exactly that.

- **Built-in blocks only.** No terminal, no shell, no `html`, no plugins.
- **Declared parameters, not free input.** The viewer doesn't edit SQL; they move a slider
  between author-set bounds, or pick from an author-set list. The viewer is exercising a
  finite state space the author already enumerated — which is Voila's widget model.
- **Bound parameters, never interpolated.** Injection stops being a category, the way it
  does for Datasette.
- **Read-only handles on scratch copies.** Datasette's immutability, applied to whatever the
  block reads. "Re-run this" should be boring by construction.
- **Caps: timeout, row limit, concurrency, per-viewer rate limit.** Read-only still permits
  an expensive query, and the machine being DoS'd is your laptop.
- **Audit every viewer-triggered run** — a row in `outputs` with the viewer's tailnet
  identity. If something surprising ran, you can see who and what.

That combination is defensible. Note what it really is, though: at T3 the author has built a
small app and the viewer is using it. That's a fine thing to be, but it's a different product
from a plan, and it shouldn't gate the first version.

## Never shared, at any tier

State this as a rule rather than a judgement call, because judgement calls erode:

- **Terminal/PTY and shell blocks.** A shared PTY is a shared shell. sshx exists and does
  this properly; if that's what's wanted, use sshx deliberately rather than getting it as a
  side effect of sharing a document.
- **Service blocks.** Proxying a section of a live project to a remote viewer exposes that
  app, with your data and your credentials behind it, to whoever holds the link — and the
  app was never written to be exposed. Service blocks are local and T1 only. This is an
  uncomfortable rule, because service blocks are the most compelling thing in a plan and T0
  can only show them as a still capture; T1 packing is the answer where sources are
  portable, and where they aren't, the honest position is that some prototypes have to be
  demonstrated in person.
- **Anything that mutates state** — writes, migrations, deploys. Author-time only.
- **Third-party plugin blocks**, pending a separate decision about reviewing plugin code.
  Agent-authored `html` and `markup` blocks are fine live, per above.

## Disclosure obligation

Sharing must be loud. `sideview share` should print exactly what it is exposing and to whom,
require an explicit flag per tier (no tier is the default), and the shared page itself should
carry a visible banner naming its tier and what a viewer can and cannot cause to happen. The
author needs to know what they just did; the viewer needs to know what they're touching.

## Recommendation

**Build T0 and T1 for v1.** Between them they cover most real sharing, need almost no new
machinery, and carry no execution risk worth managing.

**Make T2 possible without building it yet**, via two cheap decisions taken now:

1. **Every block declares whether it runs at author time or view time** — Quarto's execution
   contexts, adopted as vocabulary in the spec. Without this distinction recorded per block,
   read-only mode can't be implemented later without revisiting every block type.
2. **Parameters are typed and enumerated from the start** — a param is a name, a type, and a
   domain, not a free string. Retrofitting bounds onto free-text params later means
   redesigning every interactive block.

Neither costs anything now. Both are load-bearing for T2 and T3, and skipping them is the
kind of shortcut that closes the door quietly.

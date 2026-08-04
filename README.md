# sideview

**A visual side channel for CLI agents.** A local daemon and a companion page that let an
agent *show* you something — a plan, a 40,000-row table, a screenshot, a diff, a running
piece of your project — without that content passing through the model's context.

The name is the thesis: a view *beside* your terminal, fed by a *side channel* that bypasses
the model's context.

**Start with [V0.md](V0.md)** — the scope actually being built, which is a small fraction of
what the rest of these documents describe. Architecture notes in [DESIGN.md](DESIGN.md) ·
sharing in [SHARING.md](SHARING.md) · survey in [PRIOR-ART.md](PRIOR-ART.md).

---

## The problem

CLI agents are the right shape for real work and the wrong shape for looking at things.
Everything an agent wants to show you has to survive the terminal, and most things don't:
a wide table wraps into mush, an image can't be displayed at all, a diff is walls of
green and red, a plan is a column of markdown.

The workarounds all share one flaw. Whether the agent prints output to the terminal, writes
an HTML artifact, or returns a rich MCP-UI component, **the content passes through the
model's context on its way to your eyes.** That has three consequences:

- **It costs tokens proportional to what you're looking at.** A 40k-row table is not
  expensive to display; it is expensive to *relay*. So it never gets shown.
- **It's slow.** You wait for the model to generate the thing, at generation speed, even
  though the model has nothing useful to add to the pixels.
- **The model is a lossy bottleneck for data it doesn't need to see.** It has to read the
  table in order to show you the table.

Rich HTML artifacts are the current best answer and they inherit all three problems, plus
one of their own: the agent writes the entire document from scratch every time, which is
why they feel so slow. And an artifact can only ever *depict* a system unless that system
happens to be a web page — for a Python pipeline, a Postgres migration or a Rust CLI, it
draws a picture and stops.

## The idea

Run a small **local daemon** that owns a page in your browser, and give the agent a **CLI**
that tells it what to display. The content itself is fetched and rendered by the daemon.

The pivotal property:

> **The model names the thing. The daemon fetches and renders it.**

Naming `readings.parquet` costs the agent about ten tokens and shows you forty thousand
rows. The data never enters the conversation. Neither does the screenshot, the query
result, or the log stream. The model stays in charge of *what* to show and out of the
business of *carrying* it.

(In v0 this is a design rule rather than a shipped command — the `show` subcommand that
first carried the idea was cut, reasons in V0.md, and the first block type to exercise the
property will be `table`. Images already work today: `<img src>` in a markup block.)

Because there's a daemon rather than a file, anything the machine can run can be rendered
into the page: the real query against the real database, the real CLI, the real container.
And the strongest form of that is embedding **a section of the live project itself** — the
actual dev server, service, or binary, supervised and reachable from inside the page, in
whatever language it happens to be written in. Not a captured output; a working piece of the
thing.

## Two surfaces, one machinery

**The scratch stream** is the visual scrollback for a session: point the CLI at a thing, a
view appends, and it's on screen immediately. No setup, no document, no ceremony — this is
the visual equivalent of piping something to `less`, and it's what makes the tool worth
having on day one.

**Plans** are the flagship: a named, ordered, durable document mixing prose with those same
live views. This is where the side channel earns the most, because a plan's job is to make a
proposed change believable, and a claim with the evidence running underneath it is a
different kind of argument from a claim you're asked to take on faith. A plan is a curated
scratch stream that someone bothered to write around.

Same blocks, same daemon, same renderer. The difference is only whether anyone named it and
kept it.

## What changes

**Claims come with evidence attached.** "The new index makes this query viable" is a
sentence you have to trust; the same sentence above two real `EXPLAIN ANALYZE` outputs with
real timings is one you can check in five seconds.

**The plan and the spike merge.** "Write me a plan" and "build me a throwaway prototype so I
can see if this works" stop being two requests producing two artifacts, one of which gets
abandoned unexplained. The prototype lives inside the argument for it, next to the
alternatives it beat.

**It's a conversation, not a broadcast.** A static page is read-only. A daemon can take
input: change a parameter, paste your own awkward row, pick option B, comment on a specific
block. That comes back to the agent as data. The plan is where the loop happens rather than
what's handed over at the end of one.

**Output appears as it's produced.** Blocks patch individually, so a plan fills in while
it's being written instead of appearing once complete — and revising one block re-renders
one block. Any tool that renders a document only when finished is stuck with the latency you
already dislike.

## Concretely

**A Postgres migration.** Two panels side by side, each running `EXPLAIN ANALYZE` against a
scratch copy of the real database — current schema and proposed — with real timings, and a
slider for row count so you can find where the proposed design stops working.

**A data-pipeline parsing change.** Twenty genuinely representative rows pulled from the
actual source file, current output beside proposed, mismatches highlighted. Edit the rule
and the table re-renders because a real Python process re-ran it. Reviewing the plan *is*
testing the change.

**A CLI's ergonomics.** A real terminal attached to a real build of the binary, three
example invocations one click away, and the argument for the design written around it.

**Or just: a screenshot on the page** — `<img src="screenshot.png">` in one markup block.
Not everything needs to be a plan.

## Aim: no ceremony

**Once installed, this must be at least as easy as an HTML plan is today.** That flow is the
bar: the agent produces a thing, you get a link, a browser shows it. No daemon to start, no
port to remember, no second command, no waiting.

Concretely — the agent runs one command, a browser window appears with the content in it, and
the daemon starting up is invisible. If anyone ever has to type `sideview start`, the aim has
been missed.

**v0 meets this unsandboxed, and gets close under a sandbox** — where spawning a listener the browser
can reach is the one thing an agent cannot do, so the agent instead offers to start the daemon in a
non-sandboxed call and you approve it once per project. One prompt, nothing to type, and no
`sideview start` anywhere. The honest caveat is that the prompt exists at all, and that if you decline
it the CLI falls back to printing the command for you. See [V0.md](V0.md)'s "Starting the daemon".

The bar is beatable, though, because of where the time actually goes. Opening an HTML plan
costs a browser launch **plus waiting for the model to write the whole document** — tens of
seconds before there is anything to look at. Sideview's added cost over writing a file is a
daemon start measured in tens of milliseconds; the browser launch is identical either way; and
the first block appears as soon as it is named rather than after the last line of the document
is generated. So the target isn't parity, it's **never slower to first pixel, and no extra
steps.**

And in the case that actually matters here — agent on a remote box, browser on the laptop —
the HTML flow is worse than it looks, because every new artifact has to be fetched or
re-served before you can see it. A stable URL that patches itself means one bookmark, forever.

The engineering requirements this imposes are in [DESIGN.md](DESIGN.md); several are easy to
get wrong in ways that would feel bad rather than fail loudly.

## The shape of the answer

A plan is an ordered list of typed blocks — Notion-style, mixing prose, markup and embedded
apps — stored as rows in a **SQLite database** in the project's `.sideview/` folder. The
agent authors through a CLI writing to that file, which keeps it working under an agent
sandbox that blocks localhost daemons and sockets. A long-lived local daemon watches the
store, patches the open page in place, supervises any processes blocks depend on, and writes
the reader's comments and choices back for the agent to pick up. Common blocks — a real data
table above all — ship with the tool so no agent writes one again. Access is the Jupyter
model: localhost bind plus a per-plan token, with Tailscale for reaching a colleague.

## The hard parts

**Block specs must reference, never embed.** The entire context saving evaporates the moment
an agent inlines data into a spec instead of pointing at it. This is a design rule with teeth
— see [DESIGN.md](DESIGN.md).

**Execution authority** is less frightening than it first appears — the agent authoring a
plan already has shell access, so an embedded terminal escalates nothing. The real risks are
a reader clicking something with consequences and a plan outliving the intent behind it, both
addressed by disclosure and lifetime rather than confinement.

**Sharing** is genuinely constrained by the local-execution premise, and the useful answers
are snapshots and shipping the SQLite file rather than remote access. See
[SHARING.md](SHARING.md).

**The right level of abstraction.** Too declarative and the agent can't express what it
needs; too open and we're back to hand-rolling HTML per view. Finding the small set of block
types that covers most of what people want to see is most of the design work.

**Knowing when not to**, with a sharp test: **if a view has no live blocks, markdown was
already the right answer.** All the value is in the parts that run or the parts too big to
relay. A plan that is only prose is a markdown file with a daemon attached, and worse than
the markdown file.

## Status

Nothing built. Scope for the first version is cut down to prose and HTML blocks in
[V0.md](V0.md); everything else here is backlog and rationale, kept so the reasons behind
each decision survive.

[PRIOR-ART.md](PRIOR-ART.md) also names two things worth trying before writing code, because
part of this pain already has a solution.

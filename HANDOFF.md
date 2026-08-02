# Handoff

Written 2026-08-02, at the end of the design conversation that produced this repo. Everything here
is either current state or something that exists nowhere else in the docs.

## State

Nothing is built. Six documents, an MIT licence, one commit on `main`, no remote.

The design is settled enough to start coding from [V0.md](V0.md). It went through three reframes
getting there — from "richer plans than markdown", to "embed live pieces of a project", to "a
visual side channel for CLI agents whose content bypasses the model's context". The third is the
one the docs are written around, and it is the one that explains why the design looks the way it
does. Plans are now the flagship *use case*, not the definition.

## Do these before writing code

Both are cheap and both could change what gets built.

**Set up kitty's graphics protocol.** Displaying images in the terminal is available today at zero
build cost, and one of the motivating complaints was that seeing a screenshot from a CLI agent is
painful. Find out how much of that pain has an existing answer before building software for it.

**Spend an afternoon on the service-block spike.** Throwaway code: can a dev server be started,
proxied, iframed into a page, and killed cleanly? Service blocks are cut from v0 but they are the
long-term thesis, and this is the only experiment that could reshape the roadmap. Doing it while
nothing depends on it is much cheaper than discovering it in month three.

Optionally, an hour with [Wave Terminal](https://github.com/wavetermdev/waveterm) — its `wsh`
drives graphical blocks from the shell and is the closest existing thing to this idea. It was
rejected because the display is bound to its client app, but the ergonomics are worth feeling.

## Not documented anywhere else

**A host proxy is reachable from inside the sandbox.** The agent environment sets
`CLAUDE_CODE_HOST_HTTP_PROXY_PORT` and `CLAUDE_CODE_HOST_SOCKS_PROXY_PORT`. If the SOCKS proxy
forwards arbitrary localhost connections, a sandboxed CLI could reach the daemon directly — which
would make several of v0's workarounds unnecessary (store-based liveness, polling for change
notification, possibly the hand-started daemon itself). **Worth a ten-minute test before v1.** It
was found too late in the conversation to act on, and the store-based design works regardless.

**Naming research, so it isn't repeated.** `sideview` is free on crates.io. Also checked:
`showme` is taken by a terminal image viewer (adjacently confusing), `vitrine` by a static site
generator, `glance` by a computer-vision crate. `agentview` and `viewfinder` are free on crates.io,
but `agentview` is badly crowded — `agentview/agentview` is a session viewer for conversational
agents and `kenn-io/agentsview` does session analytics for coding agents, both of which are the
"transcript mirror" category sideview is explicitly *not*. The name was chosen to encode the
thesis: a view beside your terminal, fed by a side channel.

**The crate name is unclaimed and the repo has no remote.** Publishing is deliberately left to a
human decision. `gh repo create` when ready.

**LICENSE says "David Raznick" personally**, not Global Energy Monitor. Change it if that's wrong —
it was a judgement call based on this being a personal project directory.

**`~/.claude/skills/hunk-review` is currently a broken symlink** (into a `hunkdiff` npm package
that has moved). Noticed while researching how to ship sideview's own skill, which copies that
distribution model. Worth fixing independently.

## Open, and deliberately so

**Scroll behaviour when a block arrives.** Likely answer: scroll only when already at the bottom.
Left unspecified because it wants a real page in front of you.

**The `sv-` class list.** Six to ten classes for what Pico and Bootstrap naming don't cover —
metric/delta, option cards, decision matrix. Needs designing against real plans, not in the
abstract.

**Build order in DESIGN.md predates the v0 cut.** It reads `html` → `service` → `diff` → `table`;
V0.md supersedes it, and service blocks are out. Reconcile the two when v0 is done rather than now.

## The conversation's own summary

If a future session wants to know *why* rather than *what*, the reasoning is in the docs rather
than in any transcript — every significant decision was written down with the alternative it beat.
That was deliberate. The most load-bearing pieces of reasoning, in rough order:

1. Content must not pass through the model's context. Everything else follows.
2. The sandbox gives each Bash invocation its own network namespace, so the agent cannot start a
   reachable daemon.
3. A design vocabulary only saves tokens if the model already knows it.
4. If a page has no live blocks, markdown was already the right answer.

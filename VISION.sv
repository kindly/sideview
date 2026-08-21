<sv-page label="vision: the north star" category="plan" order="1">

<sv-prose id="north-star">
# Vision

**A CLI agent's plan deserves what a pull request gets: proper rendering and proper
review. So the plan is a live page beside your terminal: a file in the repo, commented
from the browser, picked up by the agent, still true when the work is done.**

Settled 2026-08-19, through a grill run on its own machinery. The hook, in the author's
words: CLIs need visuals, especially when planning, and they should offer proper review,
like a pull request.

## Who this is for

Designed for n=1: every design decision answers to the author's daily agent work.
Packaged for the niche: README, releases and docs answer to a CLI-agent power user who
arrives once. Mass appeal is not a constituency. Success is sideview staying the daily
driver; external users are a bonus; the design docs double as a reputation artifact.
Marketing sits at roughly zero, possibly one timeboxed launch someday; re-fronting the
README waits for that day.

## Where this sits (surveyed 2026-08-19)

The comment-loop space got crowded in 2026 (Plannotator, crit, difit, Anthropic's
Ultraplan and Claude Code Artifacts), and all of it is review-session-shaped: an
ephemeral page spawned per event, rendering content the model typed out. Uncontested,
and exactly this project's shape: a persistent daemon-owned document, data reaching the
page by reference, a plan-shaped styled vocabulary, harness independence. "Living plans"
as branding is unclaimed. The older ground is PRIOR-ART.md's.
</sv-prose>

<sv-prose id="primary">
## The primary list

What the vision pulls toward, drawn from IDEAS.sv, which keeps everything; this list
only ranks. Version curation ranks candidates by distance from the north-star.

- **The ask block (`sv-ask`)** — the loop running agent→user, answered at the block.
- **Editing prose blocks from the page** — the page's first authoring power.
- **Daily-driver ergonomics** — `sideview restart`, `--project`/store identity.
- **`_sv_cell_<col>` marks** — completing the annotated-CSV design; cell-level data
  diffs are a daily need (author, at curation).

Cut to this by the author, 2026-08-20: the list is what gets built next, not what the
vision smiles on. Deliberately not primary, with the reason attached: the **git
cluster** (changesets as page sources, `l:` line anchors, watched diffs, the Rust
anchorHash twin) waits for a named trigger — hunk stops sufficing for review at n=1
(author, 2026-08-19). Trimmed in the same cut: sv-note at its anchor, per-line comments
in code blocks, patch-in-place rendering, sv-tree, snapshot T0, sidecar log tailing.
**Service blocks** stay the long-term thesis, waiting on the never-run spike, still the
one experiment that could reshape this list. Tables beyond the marks, extensions, MCP
and sharing T1–T3 stay pool business.
</sv-prose>

</sv-page>

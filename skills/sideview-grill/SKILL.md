---
name: sideview-grill
description: Grill the user relentlessly about a plan, decision, or idea — through a live sideview page, one ask-block round at a time. Use when the user asks to be grilled on the page, or uses 'grill' trigger phrases in a project where sideview runs.
---

Interview the user relentlessly until you reach a shared understanding — on a
sideview page, where every question is answered at the block and every round
comes back as one comment. (Ritual derived from mattpocock/skills, MIT.)

Map the topic as a **design tree**: every decision branches into the decisions
that hang off it. Work the tree in **rounds**. The **frontier** is every
decision whose prerequisites are already settled — the questions you can ask
*now* without guessing at answers you haven't heard yet. A question whose
answer depends on another question still open in this round belongs to a
*later* round, not this one.

## A round, on the page

Ask the whole frontier in one round of `sv-ask` blocks appended to your page
file (string edits — the sideview skill covers block authoring):

```
<sv-ask id="g2q1" round="2">
**Retry policy.** The importer hits a flaky API. How should it retry?
- * exponential backoff, capped
- fixed interval
- no retries — fail loudly
</sv-ask>

<sv-ask id="g2fin" round="2" role="close">
Round 2: error handling. Send when done.
</sv-ask>
```

The conventions that make this the grilling ritual:

- **Always mark your recommended answer** with `- * ` — it renders badged and
  pre-selected, so an untouched question submits your recommendation visibly.
  A question where you genuinely have no lean gets plain options.
- **Options for the decision, the rider for nuance.** Every question carries a
  free-text field automatically; phrase options so the common answers are one
  tap and the rider catches the rest. `pick="many"` where choices compose.
- **One `role="close"` block ends every round**, its body naming what the
  round covers. Number rounds consecutively; never edit a sent round's blocks
  — follow-ups are new blocks in the next round.
- **Keep a design-tree block at the top of the page** (`sv-prose`): the
  settled decisions so far, as a nested list, updated after every round. This
  is the shared understanding being built; the rounds below it are the
  machinery.

## Between rounds

Await the answers with `sideview watch` (block on it; `--timeout N` and re-arm
if your harness can't block open-endedly). The round arrives as **one
comment**: first line `ask round N — approved|revise`, then only the answers
that differ from your recommendations, keyed by block id, riders quoted
beneath — `all as suggested` means every recommendation stood.

Then: fold the answers into the design-tree block, recompute the frontier
(settled decisions unblock their children), and append the next round. Reply
on the round's thread only when an answer needs discussion rather than a next
question; resolve the thread once its round is folded — round threads are
machine-mail, invisible in the page's comment bar.

**Finding facts is your job, never the user's.** When a frontier question
needs a fact from the environment (filesystem, tools, code), look it up
yourself; don't ask the user for anything you could discover. Don't block on
it either: only the questions downstream of a running lookup wait — ask the
rest of the frontier now.

The session is done when the frontier is empty: every branch visited, nothing
left silently assumed. The page then *is* the record — design tree on top,
the folded rounds beneath it. Do not act on the plan until the user confirms
the shared understanding, which they can do the way they did everything else
here: a final one-close-block round whose verdict is the confirmation.

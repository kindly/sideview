<sv-page label="v6: more than one commenter" category="plan" order="3">

<sv-prose id="intro">
# v6: more than one commenter

v5 shipped as 0.5.0 (2026-08-26); this page opened the same day. v6 is the n>1 experiment
the conversation library was pulled forward for (V5 thread 117): **n>1 is commenters,
never authors.** Focus: multi-user commenting — a page shared by link, commenters who can
name themselves, feedback the agent can follow per person. Drawn from the pool's
trust/sharing cluster, bending [SHARING.md](SHARING.md)'s T2 deliberately: a capability
link plus a self-declared name instead of tailnet identity. Rationale lands in SHARING.md
when this ships; this page holds only what v6 commits to.

Carried principles, not up for re-decision: sv files are canon and the db holds what
should not be versioned · the page file has one author and everything multi-writer goes
through SQLite · reference, never embed · dogfood first.
</sv-prose>

<sv-prose id="in">
## In

- **The funnel is the share path** *(round 1)*. Loopback and your own tailnet stay open,
  tokenless, exactly as today; security begins where the public web does. `sideview
  share` opens a Tailscale Funnel in front of the daemon, mints the credential, and
  prints the public link — one command from page to shareable URL, loud about what it
  just exposed (SHARING.md's disclosure rule). `--revoke` withdraws a link; a
  funnel-origin request without a valid token is refused server-side. Tokens live in the
  db, never versioned.
- **Two link kinds** *(round 1)*: page-scoped guest links (`share --page <id>`) that
  reach exactly their page, plus one project-wide link for your own devices (/home and
  every page).
- **Names on comments** *(round 1: self-declared per browser)*. Typed once on the page,
  kept in localStorage, sent with each comment; no accounts, no uniqueness — "whoever had
  the link", honestly labeled. Migration v6 stores the name beside the author role; the
  comment bar shows it, watch events carry it, nameless behaves exactly as today.
- **The guest boundary** *(round 1: the whole conversation surface)*. A link-holder may
  comment, reply, resolve, attach, and send edit requests (proposals, not writes) — and
  never author: prose splices, block writes, page properties, and deletion are refused
  server-side by role, not by hidden buttons (Voila's rule, kept from T2).
- **Done when:** a second person, from another machine, opens the funnel link, names
  themselves, and comments — and the agent answers them through watch, name attached.
  Live-verified, per the house ritual: each piece earns a drill.
</sv-prose>

<sv-prose id="kept">
## Deliberately not touched

- **The one-author law.** Guests comment; they never write blocks. Concurrent *authoring*
  (CRDTs, multi-agent pages) stays out, as settled in V5 thread 117.
- **Hosted sharing** stays declined (SHARING.md), **T0 snapshot / T1 pack** stay in the
  pool, and **terminal/service blocks are never shared, at any tier** — that rule stands.
- **Tailnet identity (T2 as written)** remains available later; headers compose with
  links. **Agent identity in comments** (the harness dimension) stays in the pool
  *(round 1)* — the name column lands anyway, so an agent `--name` can follow cheaply.
</sv-prose>

<sv-prose id="sequence">
## Order of work

1. **The funnel check** — the pool's never-run SSE-buffering check, now load-bearing:
   stand a trickling SSE endpoint behind `tailscale funnel` and watch whether events
   arrive one at a time. Ten minutes, needs the author (funnel must be enabled on the
   tailnet, and it is a public exposure). If the funnel buffers, the share design
   reshapes before any token code is written.
2. **Names on comments** — migration v6, the page affordance, watch and the bar. Useful
   on the open tailnet today, independent of the funnel's answer.
3. **Tokens and the guest boundary** — funnel-origin detection, the two link kinds,
   server-side role enforcement.
4. **`sideview share`** — the one command: funnel up, link and creds printed,
   disclosure-loud. The done-when runs here.
</sv-prose>

<sv-prose id="cur">
## Curation round

**Settled 2026-08-26 (round 1 approved, thread 137).** The one reversal against the
draft: the tailnet stays open — the funnel is the share path, and `sideview share`
should open it and hand back link + creds in one command ("through funnel is where
security matters"). Everything else as suggested: two link kinds, per-browser names,
the full conversation surface for guests, agent identity left in the pool. Building
starts at the order above.
</sv-prose>

<sv-ask id="c1q1" round="1">
**What the link gates.** Today anything that can reach the bind (your tailnet included)
sees everything, tokenless. V0's auth trigger was "a tailnet node you don't control" —
and sharing is inviting exactly those.
- * Every non-loopback request needs a token — one rule, no LAN/tailnet carve-outs; your own phone uses a link too (the bookmark keeps it)
- The tailnet stays open as today; links only matter beyond it
</sv-ask>

<sv-ask id="c1q2" round="1">
**Token scope.**
- * Two kinds: page-scoped guest links (`share --page <id>`), plus one project-wide link for your own devices (/home and every page)
- Page-scoped only — your own phone bookmarks pages one at a time
- Project-scoped only — any link-holder sees every page
</sv-ask>

<sv-ask id="c1q3" round="1">
**Where the name lives.**
- * Self-declared per browser: typed once on the page, kept in localStorage, sent with each comment; no accounts, no uniqueness — "whoever had the link", honestly labeled
- Bound to the link: mint one per person, the name rides the token; forwarding a link forwards the name
</sv-ask>

<sv-ask id="c1q4" round="1">
**Guest verbs.**
- * The whole conversation surface: comment, reply, resolve, attachments, edit requests (proposals, not writes). Refused: prose splices, block writes, page properties, deletion
- Comments and replies only — no edit requests, no attachments, no resolve
</sv-ask>

<sv-ask id="c1q5" round="1">
**Agent identity in comments** (the pool item: every harness writes `author: "agent"`,
indistinguishable). The same migration could carry it.
- * Stays in the pool — v6 is about humans; the name column lands anyway, and an agent `--name` can follow cheaply when two harnesses collide again
- Pull it in: agents get the harness dimension in v6
</sv-ask>

<sv-ask id="c1fin" round="1" role="close">
The curation round. Approving blesses the goal — multi-user commenting via link share and
self-chosen names — and this In list, with your answers folded in; building starts at the
first bullet.
</sv-ask>


<sv-prose id="drill2">
## Drill: names on comments (step 2, built 2026-08-26)

Migration v6 (`author_name` beside the role; your store migrated with a `-pre-v6`
backup), the logic layer normalizing once for every interface, the watch event and SSE
snapshot carrying the name, the bar showing it. 78 tests green; version 0.6.0. **Round 2
settled 2026-08-26 (thread 138): revise** — the name control left the bar's title row
for a **settings menu in the header** (the theme button grew into a popover: name,
clearly labeled, and the dark mode switch); rides-everything, the normalization rules,
and the humans-only display all stood. Round 3 below drills the menu as built. The
step-1 funnel check is still open and needs you at the machine.
</sv-prose>

<sv-ask id="d2q1" round="2">
**Where the name is set.** The bar's title row grew the control: an ink "name?" (muted
when unnamed) that turns into a small inline input; click your name to change it,
clear it to go back to unnamed. The alternative was a field on every compose card —
rejected as clutter, but it is more discoverable for a first-time guest.
- * Title row, as built — one quiet control; the guest link can point at it when step 4 lands
- Per-compose-card field — discoverability beats quiet
</sv-ask>

<sv-ask id="d2q2" round="2">
**What the name rides.** It injects once, in the client's single postComment path — so
drafts, replies, *and ask-round sends* all carry it. The plan only said "sent with each
comment"; rounds weren't named. Your round verdicts would now arrive signed.
- * Everything through one path, rounds included — one rule, and a signed verdict is a feature
- Ordinary comments only; ask rounds stay unsigned
</sv-ask>

<sv-ask id="d2q3" round="2">
**Normalization lives in the logic layer, once.** Inner whitespace collapses, a
whitespace-only name is unnamed, 60 chars is the cap (a pasted paragraph stays out of
the meta line). The daemon accepts anything and normalizes server-side — the browser's
maxlength is a courtesy, not the truth.
- * As built (this question is the record)
- Different rules — rider says what
</sv-ask>

<sv-ask id="d2q4" round="2">
**The display rule.** The meta line shows the name only for humans: `agent` stays
`agent` even if a name were ever posted with it, the ink treatment still keys off the
role, and a nameless comment reads exactly as today (`user`). Old rows are NULL —
pre-v6 shape, byte for byte.
- * As built — the name sits beside the role, never instead of it
- Agents should be nameable too (pulls the pool's harness-identity item forward)
</sv-ask>

<sv-ask id="d2fin" round="2" role="close">
Drill round 2: the names piece. The new bar is embedded in the binary, so to try it run
`sideview restart`, set a name in the bar's title row, and comment anywhere — the watch
event will carry it. Approving closes step 2; step 3 (tokens + the guest boundary)
waits on the step-1 funnel check, which needs you.
</sv-ask>


<sv-prose id="drill2b">
## Drill: the settings menu (round 2's revision, built 2026-08-26)

As ordered: the header's theme button is now ⚙, opening a small popover — **Name**,
clearly labeled, and a **Dark mode** switch (native `details`, no bootstrap JS; closes
on outside click, Esc, or Enter in the name field). A pleasant find en route: debug
builds serve `static/` from disk, so the menu is already on your page after a reload —
but *saving a name with a comment* still needs `sideview restart`, because the running
daemon is the pre-name binary and silently drops the field.

**Round 3 approved 2026-08-26 (thread 139), two riders folded the same hour: auto is
dropped** — a first visit follows the OS silently, the first flip stores light/dark
and that is the whole model (no explicit auto state, no reset line) — **and a first
visit starts with the menu open**, so the name field introduces itself to a guest who
just followed a link (once per browser; any interaction closes it). The ⚙ stood.
**Step 2 is closed**; what remains of v6 is the step-1 funnel check, then tokens and
`sideview share`.
</sv-prose>

<sv-ask id="d3q1" round="3">
**The switch is binary; the theme model is not.** Today's model is auto → light → dark.
As built: the switch shows the *effective* mode; flipping it pins an explicit override,
and a muted "theme: follow the system" line appears in the menu only while pinned —
flip-and-forget never strands you, auto stays reachable. The alternative was dropping
auto entirely.
- * Keep auto behind the switch, as built — the OS follower is the right default and the reset is one muted line
- Drop auto: the switch is the whole model (light or dark, remembered)
</sv-ask>

<sv-ask id="d3q2" round="3">
**The header glyph is a static ⚙.** The old button wore the theme state (◐/☀/☾); that
reading now lives only inside the menu, as the switch's position. One less thing in the
header, one click further to see the state.
- * ⚙, as built (this question is the record)
- Keep the state glyph on the summary button (⚙ only on hover / never)
</sv-ask>

<sv-ask id="d3fin" round="3" role="close">
Round 3: the settings menu. Reload to see it (no restart needed for the looks); run
`sideview restart` before testing that a comment actually carries your name — then set
the name in ⚙ and comment anywhere, and the watch event arrives signed. Approving
closes step 2 for real.
</sv-ask>

</sv-page>

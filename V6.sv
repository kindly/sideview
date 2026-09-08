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


<sv-prose id="funnel">
## The funnel check (step 1, run live 2026-08-27)

The question standing since v0 is answered: **the funnel does not buffer SSE.** A
throwaway one-tick-per-second probe behind `tailscale funnel`, measured on both paths —
tailnet-direct through the serve proxy (gaps ~1000 ms, transit ≤2 ms) and the true
public relay through the ingress, resolved by public DNS (all 30 events, gaps ~1000 ms,
transit ≤28 ms). Funnel and probe torn down after; the public URL now refuses.
**Round 4 approved 2026-08-27, all as suggested (thread 141): step 1 closed** — share
detects-and-instructs at both consent gates, step 3 next, the cert wait handled in
share's UX. The
share design inherits four facts: funnel needs a **one-time tailnet enable** (the CLI
prints the admin URL and waits); tailscaled takes serve configs only from root or its
**operator** (`sudo tailscale set --operator=$USER`, once); funnel is **TLS
passthrough** — the certificate lives on this machine, and the *first* HTTPS hit waits
~30 s on its issuance (our first relay run timed out on exactly that); and the funnel
proxies **one hostname to one port** — whatever it points at is entirely public.
</sv-prose>

<sv-ask id="d4q1" round="4">
**How `sideview share` handles the two consent gates.** Funnel enablement and operator
mode are both one-time, both outside sideview's authority.
- * Detect and instruct: on denial, print the exact one-time command / admin URL and stop — sideview never sudos, never opens the browser to the admin console itself
- Drive it: share runs `sudo tailscale set --operator` itself and opens the enable URL
</sv-ask>

<sv-ask id="d4q2" round="4">
**Sequencing confirmed by the one-port fact.** The funnel exposes the whole daemon, so
tokens and the guest boundary (step 3) must be live before `share` ever points funnel
at a real sideview port; and share's first run should expect the ~30 s certificate
wait (warm it, or say so honestly).
- * Confirmed — step 3 next, share last, cert wait handled in share's UX
- Something reads wrong — rider says what
</sv-ask>

<sv-ask id="d4fin" round="4" role="close">
Round 4: the funnel check's findings. Approving closes step 1 and starts step 3 —
tokens and the guest boundary.
</sv-ask>


<sv-prose id="step3">
## Step 3 design: tokens and the guest boundary

Before code, the shape. **Origin**: a request wearing tailscaled's funnel mark
(`Tailscale-Funnel-Request: ?1` — verified live 2026-08-27 with a header-echo probe;
a relayed request also carries the caller's real public IP in X-Forwarded-For, and a
serve-proxied *tailnet* request instead carries `Tailscale-User-Login`/`-Name`, T2's
identity headers, already flowing) is outside; everything else — loopback,
tailnet-direct, serve-proxied tailnet — stays open exactly as today. The relay also
takes ~15–20 s to start routing after each enable — share's UX inherits that wait
beside the cert one. A local process forging the mark only locks itself out. **Tokens**:
migration v7 adds a `shares` table (unguessable token, page or NULL for project-wide,
created/revoked stamps) — db, never versioned. **Roles**: funnel with no valid token is
one honest 403; a page token is the guest role on exactly that page (the conversation
surface, per round 1); the project-wide token is *your* link, so it carries the owner
role — everything your local browser can do, editing included. Enforcement is
server-side by role at every endpoint; hiding buttons is UX on top. Round 5 settles the
three open forks.
</sv-prose>

<sv-ask id="c5q1" round="5">
**How the token travels.** The guest link should be one URL that keeps working.
- * Query once, cookie after: the link is `/p/<id>?k=<token>`; the daemon sets a Secure HttpOnly cookie and redirects clean; SSE, api and assets ride the cookie; the bookmark re-sets it each visit
- Token in every path (`/share/<token>/…`) — no cookies, but every asset and api URL needs rewriting
</sv-ask>

<sv-ask id="c5q2" round="5">
**What a guest's SSE connection carries.** Live updates are half the product, but the
stream must not leak the rest of the project.
- * The same SSE endpoint, role-filtered server-side: a guest receives only their page's block and thread events; the page-strip event is withheld (other pages' existence doesn't leak)
- No SSE for guests in v6 — a static page plus comment posting; live view arrives in a later version
</sv-ask>

<sv-ask id="c5q3" round="5">
**The file endpoint for guests.** `/f/` serves any project file (root-confined) —
right for you, a project-wide disclosure for a page-scoped guest.
- * Guests get `.sideview/attachments/` only: comment images work; a markup block's `<img src="/f/shots/x.png">` breaks for guests in v6 — an honest, recorded gap (the owner token keeps full `/f/`)
- Any valid token gets full `/f/` — handing someone the link is trusting them with what the page might reference
</sv-ask>

<sv-ask id="c5fin" round="5" role="close">
Round 5: the step-3 forks, before a line of it is written. Approving sets the design;
the build follows with its own drill.
</sv-ask>


<sv-prose id="drill6">
## Drill: the guest boundary (step 3, built 2026-08-27 — round 6 approved same day,
all as suggested, thread 144; step 3 closed)

Built to round 5's answers: migration v7 and the shares concept (`models/share.rs`,
token → Owner / Guest-on-one-page), the funnel gate on every route (`?k=` adopted into
a Secure HttpOnly cookie with a clean redirect; a fresh `?k=` beats a stale cookie),
role-filtered SSE (a guest connection replays and receives only its page; the pages
event arrives re-scoped), the conversation surface open to guests with the page guard
on every write, authoring refused server-side, full `/f/` for any valid token, and the
shell marking guests so the client withholds the strip and the pencil. Extensions
started as a blanket guest refusal; the author's whitelist design replaced it the same
day, and the same thread then re-founded the extension body contract itself — a
mapping, always, YAML 1.2 with JSON valid by superset (thread 143 — d6q1 below has the
shape; EXTENSIONS.md carries the contract; serde_norway is the pinned fork). 83 tests,
the gate pinned end to end (tokenless 403, adoption redirect, cross-page refusals,
whole-call whitelist matching in both spellings, owner-full, revocation immediate).
</sv-prose>

<sv-ask id="d6q1" round="6">
**Extensions for guests: the `_sv_allow` whitelist, and the body became a mapping**
*(your design, thread 143, built same day)*. Extension-block bodies are now always a
mapping — parsed as YAML 1.2, so plain JSON is equally valid (your superset point; no
free-text form). The daemon parses once and injects `SIDEVIEW_BLOCK.config`; frames
need no YAML library. A guest's browser gets the frame — the block renders — and a
`call_cli` passes only on an exact whole-call match:

```
query: |
  select region, sum(mw) from plants group by 1
_sv_allow:
  - args: [-jsonlines]
    stdin: "select region, sum(mw) from plants group by 1"
```

`stdin` matches too (args alone would leave a SQL tool's stdin open to anything); a
non-mapping body allows nothing; the list lives in canon, so a guest can never widen
it; owner link and tailnet stay unrestricted. Explicit entries now, patterns later.
Both reference extensions and the demo page migrated (`query:` / `cmd:` keys).
- * As built — execution bounded by construction, and one body contract instead of two
- Something reads wrong — rider says what
</sv-ask>

<sv-ask id="d6q2" round="6">
**One refusal, three causes.** No token, a revoked token, and a valid token asking for
the wrong page all get the identical 403 page — a capability URL reveals nothing about
what else exists, not even "that page is real".
- * As built (this question is the record)
- Differentiate the messages
</sv-ask>

<sv-ask id="d6q3" round="6">
**`/assets/` stays tokenless.** The static css/js answer any funnel caller — needed
before any styled page can render, importable by the opaque-origin islands, and
content-free (the same bytes ship in every copy of the binary).
- * As built — static assets are public artifacts, not project data
- Gate them too: nothing answers without a token
</sv-ask>

<sv-ask id="d6fin" round="6" role="close">
Drill round 6: the guest boundary. To feel it live before approving: `sideview
restart`, then I mint a guest token for this page, you run `tailscale funnel --bg
<port>`, and the link opens this page — and only this page — from any browser off the
tailnet (your phone with wifi off is the honest test). Approving closes step 3;
step 4 (`sideview share`, the one command) is last.
</sv-ask>


<sv-prose id="drill7">
## Drill: `sideview share` (step 4, built 2026-08-27)

The one command, to round 4's law. `sideview share --page <id>` mints (or hands back)
the guest link, brings the funnel up, and prints the URL with exactly what it exposes;
bare `share` is the owner link (`/?k=…` — the daemon adopts it and lands you on the
most active page); `--revoke <token-or-URL>`, `--list`, `--off` (funnel down, links
keep). At each consent gate it prints the exact one-time command or admin URL,
composed to be relayed verbatim, and stops — never sudo, never the browser. The skill
now says share is the user's command, like restart.

**Round 7 approved 2026-08-29 (thread 145), all as suggested — the three decisions
stand and step 4's code is committed. The done-when itself is still open, honestly:**
at approval time no link had been minted, the funnel was off, and the daemon predates
the share verb. Two commands when you're ready — `sideview restart`, then `sideview
share --page V6` — and the phone test (wifi off, set a name, comment) closes v6's
done-when for real. The release round follows that, not this.
</sv-prose>

<sv-ask id="d7q1" round="7">
**Share is idempotent per scope.** Sharing the same page twice hands back the same
URL — a fresh link is revoke-then-share. The alternative (every share mints anew)
multiplies live tokens nobody remembers.
- * As built — one live link per scope, deliberate revocation
- Every share mints fresh; old links keep working until revoked
</sv-ask>

<sv-ask id="d7q2" round="7">
**`tailscale funnel --bg` polls forever at the enable gate** (observed live), so
share gives the child 5 s, then kills it and judges the transcript — Up, or a Blocked
message carrying the admin URL / operator command verbatim. The three transcripts
seen live are test-pinned.
- * As built (this question is the record)
- Different timeout / approach — rider says what
</sv-ask>

<sv-ask id="d7q3" round="7">
**Share works with the daemon down** — it warns ("the link answers once one is") and
proceeds, because the funnel and the mint don't need the daemon, only its remembered
port. The alternative was refusing outright.
- * As built — warn and proceed; a link minted before the daemon starts is still a good link
- Refuse without a live daemon
</sv-ask>

<sv-ask id="d7fin" round="7" role="close">
Drill round 7, and v6's done-when: run `sideview restart` (the running daemon predates
the gate), then `sideview share --page V6` and open the printed link from your phone
with wifi off — set a name, comment. The event should reach me signed, through the
funnel, and my reply should land on your phone live. Approving closes step 4 and
starts the release sweep.
</sv-ask>


<sv-prose id="donewhen">
## The done-when, met live (2026-09-08, thread 148)

One test session walked every leg, each verified against the store before being
believed: **names** (foo, david, No tailnet — signed comments, answered by name) ·
**the funnel transport** (a comment that could only have ridden a token, off-tailnet,
through the public relay) · **the owner link** (a direct prose splice into this page's
title from the off-tailnet browser — "woo") · **revocation** (both owner tokens
stamped, the browser met its 403, rows kept as audit) · **the guest boundary** (the
page-scoped link commenting from off the tailnet, strip and pencil withheld,
everything else refused). Two findings taught along the way: the ts.net URL is *the
same in both worlds* — a phone running the Tailscale app rides the open tailnet path
even on cellular, so the honest guest test is Tailscale-off — and **an SSE stream
opened before a revocation keeps flowing until it reconnects** (access is checked at
connect; the next comment, reload or reconnect is refused). Round 8 settles that
finding and orders the release.

**Round 8 approved 2026-09-08 (thread 150): the SSE-revocation behavior is accepted
for v6 and its fix pooled; the "woo" tidied out of the title; and the approve is the
release order — v0.6.0.**
</sv-prose>

<sv-ask id="d8q1" round="8">
**Revocation and live streams.** A revoked guest's already-open tab keeps receiving
its page's updates until the connection drops; every new request is refused. Severing
live streams on revocation means the gate re-checking per event or a
generation-stamped connection registry.
- * Accept for v6 and pool the fix — revocation of *new* requests is the security line; a stream is at most one tab-lifetime of already-granted reading
- Fix in v6: revocation severs live SSE connections too
</sv-ask>

<sv-ask id="d8q2" round="8">
**The "woo".** The remote-splice proof left the page title reading "more than one
commenter woo" — your edit, in canon.
- * I tidy it out before the release commit (the proof lives in this record and thread 148)
- Keep it — a battle scar
</sv-ask>

<sv-ask id="d8fin" round="8" role="close">
Round 8: the release. Everything v6 committed to is built, drilled, and now
live-verified end to end; 84 tests, docs speaking the shipped design (SHARING.md's T2
carries the rationale, EXTENSIONS.md the body contract, the skill the share law).
Your approve is the order to: push main, tag v0.6.0 (release.yml builds the
binaries), publish to crates.io, and re-sync the installed skills on this machine's
harnesses. Nothing is tagged or published before it.
</sv-ask>

</sv-page>

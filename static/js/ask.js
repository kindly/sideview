// Ask blocks: drafts in this browser until the round's close block sends one
// comment through the ordinary machinery.
import { state, conversation } from './state.js';
import { $blocks } from './dom.js';
import { postComment } from './conversation.js';

// ---- ask blocks ---------------------------------------------------------------
// Answers are drafts in this browser (localStorage, per page and round,
// freely revisitable) until the round's close block sends ONE comment
// through the ordinary machinery — watch covers it unchanged. Delegated and
// rehydrated, because blocks are replaced wholesale on every SSE patch.
// Whether a round was sent is derived from the conversation snapshot (a
// thread on the close block whose quote is `ask round N`), never held
// locally — so every tab and device agrees.

function askDraftKey(round) {
  return 'sv-ask:' + state.selected + ':' + round;
}

function askDraft(round) {
  try {
    return JSON.parse(localStorage.getItem(askDraftKey(round))) || {};
  } catch (e) {
    return {};
  }
}

function askBlocks(round) {
  return [...$blocks.querySelectorAll('.sv-ask[data-sv-round="' + round + '"]')];
}

function askPicked(section) {
  return [...section.querySelectorAll('input:checked')];
}

function saveAskDraft(section) {
  const round = section.dataset.svRound;
  const d = askDraft(round);
  d[section.dataset.block] = {
    c: askPicked(section).map((i) => i.value),
    r: section.querySelector('.sv-ask-rider')?.value || '',
  };
  localStorage.setItem(askDraftKey(round), JSON.stringify(d));
}

function hydrateAsk(el) {
  if (!el.matches?.('.sv-ask') || !el.dataset.svRound) return;
  const d = askDraft(el.dataset.svRound)[el.dataset.block];
  if (d) {
    // The draft is the whole truth once it exists — including unchecking a
    // server-suggested default the reader turned down.
    const picks = Array.isArray(d.c) ? d.c : d.c != null ? [d.c] : [];
    for (const i of el.querySelectorAll('input')) i.checked = picks.includes(i.value);
    const rider = el.querySelector('.sv-ask-rider');
    if (rider && d.r) rider.value = d.r;
  }
  updateAskControls(el);
  // A question block re-arriving alone must still learn its round is sent.
  if (el.dataset.svRole !== 'close') {
    const closeEl = $blocks.querySelector(
      '.sv-ask-close[data-sv-round="' + el.dataset.svRound + '"]'
    );
    if (closeEl) applyAskDone(el, !!askSentThread(closeEl));
  }
}

// The thread a sent round created, or undefined. The quote is the round's
// marker — set by the send below, matched here, read by the agent's watch.
function askSentThread(closeEl) {
  return conversation().threads.find(
    (t) =>
      t.target === closeEl.dataset.block &&
      t.quote === 'ask round ' + closeEl.dataset.svRound
  );
}

// Rounds the reader reopened after a send: complete, expanded for amending,
// never locked. In memory on purpose — a reopened round is a round being
// amended right now, not a preference.
const askOpen = new Set();

// A sent round folds to ONE line on its close block (round-2 drill: the
// whole round collapses, not each question, and the ink bar belongs to
// current asks only) — question blocks leave the flow entirely until
// `change` brings the round back. The summary is injected decoration, so it
// must never sit inside a p/li/pre (content hashes would see it) — a div
// appended to the section is safe.
function applyAskDone(el, done) {
  const show = done && !askOpen.has(el.dataset.svRound);
  // Settled = sent, folded or not: the ink bar belongs to rounds that still
  // want answers, so a sent round stays barless even while viewed expanded
  // (round-6 drill, thread 72).
  el.classList.toggle('sv-ask-settled', done);
  el.classList.toggle('sv-ask-done', show);
  el.querySelector('.sv-ask-summary')?.remove();
  if (!show || el.dataset.svRole !== 'close') return;
  const verdict = askPicked(el)[0];
  const s = document.createElement('div');
  s.className = 'sv-ask-summary';
  const label = document.createElement('span');
  label.textContent =
    '✓ round ' + el.dataset.svRound + ' — ' + (verdict ? verdict.value : 'sent');
  const view = document.createElement('button');
  view.type = 'button';
  view.className = 'sv-ask-change';
  view.textContent = 'view';
  s.append(label, view);
  el.appendChild(s);
}

// A round thread is machine-mail: it reaches the agent through watch, the
// folded line on the page is the reader's record, and the bar never shows
// it (round-3 drill). The agent resolves these after folding — there is no
// UI left to resolve them from.
function isAskThread(t) {
  return /^ask round \d+$/.test(t.quote || '');
}

function updateAskControls(el) {
  if (el.dataset.svRole !== 'close') return;
  const send = el.querySelector('.sv-ask-send');
  if (!send) return; // a degraded close block has no controls
  const round = el.dataset.svRound;
  const t = askSentThread(el);
  // Completion is per-round and the verdict is what completes it.
  send.disabled = !el.querySelector('input:checked');
  send.textContent = t ? 'send again' : 'send round ' + round;
  const sent = el.querySelector('.sv-ask-sent');
  if (sent) {
    sent.hidden = !t;
    if (t) sent.textContent = 'round ' + round + ' sent — an amended send lands on the same thread';
  }
  // A sent round held open by `view` offers the way back down: `hide`.
  el.querySelector('.sv-ask-hide')?.remove();
  if (t && askOpen.has(round)) {
    const hide = document.createElement('button');
    hide.type = 'button';
    hide.className = 'sv-ask-hide btn btn-sm btn-outline-secondary';
    hide.textContent = 'hide';
    el.querySelector('.sv-ask-actions')?.appendChild(hide);
  }
  // The whole round wears the sent state: complete, collapsed, never locked.
  for (const q of askBlocks(round)) applyAskDone(q, !!t);
}

function refreshAskSent() {
  for (const el of $blocks.querySelectorAll('.sv-ask-close')) updateAskControls(el);
}

// Exactly what the round comment will say — shown verbatim in the preview,
// posted verbatim on send. Readable markdown, one entry per question in
// page order, unanswered ones ridden along honestly.
// Terse by the author's order (round-2 drill), and agent-facing by their
// delegation (round 3: the bar never shows it, so the format serves the
// reader of watch): entries are keyed by question block id, and only the
// ones whose answer differs from the suggestion are listed — a suggestion
// left standing is covered by the tail line. The server-rendered `checked`
// attribute survives as defaultChecked, which is how the diff against the
// suggestion needs no server help.
function askRoundBody(round, closeEl) {
  const verdict = askPicked(closeEl)[0];
  const lines = ['ask round ' + round + ' — ' + (verdict ? verdict.value : '(no verdict)')];
  let n = 0;
  let skipped = 0;
  for (const q of askBlocks(round)) {
    if (q.dataset.svRole === 'close') continue;
    n++;
    const inputs = [...q.querySelectorAll('input')];
    const rider = (q.querySelector('.sv-ask-rider')?.value || '').trim();
    const suggested = inputs.some((i) => i.defaultChecked);
    const asSuggested = inputs.every((i) => i.checked === i.defaultChecked);
    if (suggested && asSuggested && !rider) {
      skipped++;
      continue;
    }
    const labels = askPicked(q).map((i) => i.parentElement.querySelector('span')?.textContent);
    lines.push(
      q.dataset.block + ': ' + (labels.length ? labels.join(' · ') : rider ? '' : '(unanswered)')
    );
    if (rider) lines.push('> ' + rider.replace(/\n/g, '\n> '));
  }
  if (skipped) lines.push(skipped === n ? 'all as suggested' : 'everything else as suggested');
  const note = (closeEl.querySelector('.sv-ask-rider')?.value || '').trim();
  if (note) lines.push('note: ' + note);
  return lines.join('\n') + '\n';
}

$blocks.addEventListener('input', (e) => {
  const section = e.target.closest('.sv-ask');
  if (!section || !section.dataset.svRound) return;
  saveAskDraft(section);
  if (section.dataset.svRole === 'close') updateAskControls(section);
});

$blocks.addEventListener('click', async (e) => {
  const cb = e.target.closest('.sv-ask-change');
  if (cb) {
    const el = cb.closest('.sv-ask'); // the close block — the summary lives there
    askOpen.add(el.dataset.svRound);
    updateAskControls(el); // recomputes against askOpen: the round expands
    return;
  }
  const hb = e.target.closest('.sv-ask-hide');
  if (hb) {
    const el = hb.closest('.sv-ask');
    askOpen.delete(el.dataset.svRound);
    updateAskControls(el); // recomputes: the round folds back to its line
    return;
  }
  const sb = e.target.closest('.sv-ask-send');
  if (!sb) return;
  const el = sb.closest('.sv-ask');
  const round = el.dataset.svRound;
  sb.disabled = true;
  try {
    // A round has one thread: the first send creates it, an amended send
    // replies into it — never a second thread wearing the same round.
    const t = askSentThread(el);
    await postComment(
      t
        ? { thread: t.id, body: askRoundBody(round, el) }
        : {
            page: state.selected,
            target: el.dataset.block,
            anchor: '',
            quote: 'ask round ' + round,
            context: null,
            body: askRoundBody(round, el),
          }
    );
    // The daemon's threads snapshot repaints the sent state; no local
    // state — but a round reopened for this amend folds back up with it.
    askOpen.delete(round);
  } catch (err) {
    console.warn('sideview: ask round failed to send', err);
    sb.textContent = 'failed — try again';
    sb.disabled = false;
  }
});

export { hydrateAsk, refreshAskSent, isAskThread };


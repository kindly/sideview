// Conversation plumbing: the snapshot into the island, the API calls out.
import { state, conversation } from './state.js';
import { resolveAnchor } from './anchors.js';
import { refreshAskSent, isAskThread } from './ask.js';
import { svc } from './commentbar.js';

// Recompute which threads still attach, after any block or thread change.
let conversationScheduled = false;
function scheduleConversation() {
  if (conversationScheduled) return;
  conversationScheduled = true;
  requestAnimationFrame(() => {
    conversationScheduled = false;
    syncConversation();
  });
}

function applyCbarPref() {
  // Only this page's drafts count: one written elsewhere waits on its own
  // page rather than propping the bar open everywhere.
  const draftsHere = svc ? svc.drafts.filter((d) => d.page === state.selected) : [];
  const has = svc && (svc.threads.length > 0 || draftsHere.length > 0);
  document.body.classList.toggle('sv-cbar', !!has);
  if (!has) { document.body.classList.remove('sv-cbar-open'); return; }
  if (draftsHere.length) { document.body.classList.add('sv-cbar-open'); return; }
  // Opening is automatic when warranted; closing never is (thread 8's law:
  // folding is explicit, only the chevron or the chip). So this only ever
  // adds the class — an open bar stays open through replies, sends, and
  // snapshot churn.
  if (document.body.classList.contains('sv-cbar-open')) return;
  const stored = localStorage.getItem('sv-cbar:' + state.selected);
  const wide = matchMedia('(min-width: 64rem)').matches;
  if (stored ? stored === 'open' : wide) document.body.classList.add('sv-cbar-open');
}

function syncConversation() {
  if (!svc) return;
  const conv = conversation();
  svc.page = state.selected;
  // Machine-mail never reaches the bar: ask-round threads (round-3 drill)
  // and 'edited' records (a prose splice's watch receipt) — the badge, the
  // count dots and the turn indicator all pretend they don't exist. Edit
  // *requests* (kind 'edit') stay visible: they are conversation.
  const kindOf = (t) => (conv.comments.find((c) => c.thread_id === t.id) || {}).kind;
  svc.threads = conv.threads.filter((t) => !isAskThread(t) && kindOf(t) !== 'edited');
  svc.comments = conv.comments;
  svc.attachments = conv.attachments || [];
  const attach = {};
  for (const t of conv.threads) attach[t.id] = !!resolveAnchor(t);
  svc.attach = attach;
  refreshAskSent(); // a sent round's state is derived from these threads
}

async function postComment(payload) {
  // The self-declared name (V6.sv round 1) rides every send from this one
  // spot — drafts, replies and ask rounds alike. Absent means unnamed,
  // which is exactly the pre-v6 shape; the daemon normalizes.
  const name = localStorage.getItem('sv-name');
  if (name) payload = { author_name: name, ...payload };
  const res = await fetch('/api/comments', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error(await res.text());
}

function setResolution(threadId, undo) {
  fetch(`/api/threads/${threadId}/${undo ? 'unresolve' : 'resolve'}`, { method: 'POST' })
    .catch(() => {});
  // The daemon's snapshot repaints the bar within a tick; no local state.
}

export { scheduleConversation, syncConversation, applyCbarPref, postComment, setResolution };


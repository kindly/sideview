// The one stream. Handlers only route: state in, module calls out.
import { CLIENT_STAMP, state, ROUTE } from './state.js';
import { $blocks, $status, $brand } from './dom.js';
import { readingRef, setReconnectAnchor, hideNewPill, scheduleRestore } from './scroll.js';
import { renderPageStrip, renderIndex } from './pagestrip.js';
import { refreshOutline } from './outline.js';
import { applyBlock } from './blocks.js';
import { scheduleConversation } from './conversation.js';

const es = new EventSource('/events');
es.addEventListener('open', () => {
  state.connectedAt = Date.now();
  const ref = readingRef();
  if (ref) {
    setReconnectAnchor({ block: ref.block, top: ref.top, until: Date.now() + 8000 });
  }
  // No ref means the page is empty — a fresh load. Keep whatever the
  // sessionStorage anchor parked in reconnectAnchor; an empty page has
  // nothing better to say about where the reading was.
  hideNewPill();
  // Every connection replays the full current state (the daemon keeps no
  // per-client cursor), so drop what we hold — blocks removed while we were
  // away would otherwise linger as ghosts.
  state.blocks.clear();
  $blocks.textContent = '';
  clearTimeout(deadTimer);
  deadTimer = 0;
  document.body.classList.remove('sv-disconnected', 'sv-dead');
  $status.hidden = true;
  $brand.title = 'connected · client ' + CLIENT_STAMP;
});
// Reconnecting and gone are one signal apart: EventSource retries forever,
// so "gone" is honestly a duration — after 8s of failed retries the lids
// draw. The timer must not re-arm per error event or it never fires.
let deadTimer = 0;
es.addEventListener('error', () => {
  // EventSource reconnects on its own; the reviewer goes half-lidded.
  document.body.classList.add('sv-disconnected');
  $status.hidden = false;
  $brand.title = 'reconnecting';
  if (!deadTimer && !document.body.classList.contains('sv-dead')) {
    deadTimer = setTimeout(() => {
      document.body.classList.add('sv-dead');
      $brand.title = 'daemon gone — run `sideview` in the project';
    }, 8000);
  }
});

es.addEventListener('pages', (e) => {
  state.pages = JSON.parse(e.data).pages;
  // The snapshot is authoritative: a page it doesn't list is gone, blocks
  // and all — this is how deletion reaches every open tab.
  const ids = new Set(state.pages.map((s) => s.id));
  for (const held of [...state.blocks.keys()]) {
    if (!ids.has(held)) state.blocks.delete(held);
  }
  // No auto-follow: the URL is the state, and a tab never navigates itself
  // (retired with the client router, 2026-08-18 — its pinning saga cost more
  // confusion than the feature earned). The strip still updates live, so a
  // new page is one click away, not zero.
  if (state.selected && !ids.has(state.selected)) {
    // The page under this tab was deleted: a real navigation, like any other.
    location.href = '/';
    return;
  }
  renderPageStrip();
  if (ROUTE.view === 'home') renderIndex(); // the index lists pages: it follows them
  refreshOutline(); // a page's outline property may have changed
});

es.addEventListener('block', (e) => {
  const ev = JSON.parse(e.data);
  let per = state.blocks.get(ev.page);
  if (!per) { per = new Map(); state.blocks.set(ev.page, per); }
  if (ev.action === 'remove') per.delete(ev.block);
  else per.set(ev.block, { ord: ev.ord, html: ev.html, headings: ev.headings || [] });
  if (ev.page === state.selected) {
    applyBlock(ev);
    refreshOutline();
    scheduleConversation(); // a replaced block sheds its count-dots
    scheduleRestore();      // reconnect replay: put the reading back
  }
});

es.addEventListener('threads', (e) => {
  const ev = JSON.parse(e.data);
  state.conversations.set(ev.page, {
    threads: ev.threads,
    comments: ev.comments,
    attachments: ev.attachments || [],
  });
  if (ev.page === state.selected) scheduleConversation();
});


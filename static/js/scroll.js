import { $blocks, blockEl } from './dom.js';

// ---- scroll: the reading position is sacred ---------------------------------
// The author's rule (2026-08-08, deciding the v0 scroll question on a day of
// real use): never move what the user is reading. No auto-follow — new
// content below the fold gets a floating pill instead; changes above the
// viewport are compensated so the text under the eye stays put; reconnects
// remember the block being read and put it back.

// The block currently under the reading line, and where its top sat.
// Identified by block id, never by node: replace/move mutations swap in
// fresh nodes, and an anchor held on the dead node would skip compensation
// exactly when the block being read is the one that moved (found live,
// 2026-08-08 — a cascade of moved blocks stranded the author mid-page).
function readingRef() {
  // Anchor the block under the reading line (~a third down the viewport,
  // where eyes actually sit) — anchoring the topmost visible block left a
  // blind spot: an insertion between that block and the reading line was
  // invisible to compensation and nudged the text by its height (the
  // residual small jump, take seven).
  const line = Math.max(100, innerHeight * 0.3);
  for (const el of $blocks.children) {
    const r = el.getBoundingClientRect();
    if (r.bottom > line) return { block: el.dataset.block, top: r.top };
  }
  return null;
}

// Run a DOM mutation, then counter-scroll so the block being read stays
// exactly where it was. Wholesale block replacement defeats the browser's
// native scroll anchoring, so this is our own.
function keepReading(mutate) {
  const ref = readingRef();
  mutate();
  if (!ref) return;
  const el = blockEl(ref.block);
  if (!el) return; // the reading itself was removed; nothing to hold to
  const delta = el.getBoundingClientRect().top - ref.top;
  if (delta) {
    console.debug('sideview: reading anchor compensated', delta, 'px');
    scrollBy(0, delta);
  }
}

// The pill: genuinely-new content that landed out of view, offered, never
// imposed. Click to go; it retires itself once the content scrolls into view.
const $newpill = document.createElement('button');
$newpill.id = 'sv-newpill';
$newpill.type = 'button';
$newpill.textContent = '↓ new content below';
$newpill.hidden = true;
document.body.appendChild($newpill);
let newBelowEl = null;
function hideNewPill() {
  newBelowEl = null;
  $newpill.hidden = true;
}
$newpill.addEventListener('click', () => {
  newBelowEl?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  hideNewPill();
});
addEventListener('scroll', () => {
  if (newBelowEl && newBelowEl.getBoundingClientRect().top < innerHeight) hideNewPill();
}, { passive: true });

// Across a reconnect: remember which block was being read, restore it once
// the replay burst goes quiet. Refreshes ride the same machinery: the
// browser's own restoration races the SSE stream and clamps to whatever
// height exists when its window fires (observed: always "a little way down"),
// so it's set to manual and the anchor travels through sessionStorage.
history.scrollRestoration = 'manual';
let reconnectAnchor = null;
try {
  const saved = JSON.parse(sessionStorage.getItem('sv-reading') || 'null');
  sessionStorage.removeItem('sv-reading');
  if (saved && saved.block) {
    reconnectAnchor = { block: saved.block, top: saved.top, until: Date.now() + 10000 };
  }
} catch { /* a torn save is just a top-of-page load */ }
addEventListener('pagehide', () => {
  const ref = readingRef();
  if (ref) sessionStorage.setItem('sv-reading', JSON.stringify(ref));
});
let restoreTimer = 0;
function scheduleRestore() {
  if (!reconnectAnchor) return;
  clearTimeout(restoreTimer);
  restoreTimer = setTimeout(() => {
    const a = reconnectAnchor;
    reconnectAnchor = null;
    if (!a || Date.now() > a.until) return;
    const el = blockEl(a.block);
    if (el) scrollBy(0, el.getBoundingClientRect().top - a.top);
  }, 300);
}

// New content out of view: offered, never imposed (blocks.js calls this).
function offerPill(el) {
  if (newBelowEl) return;
  newBelowEl = el;
  $newpill.hidden = false;
}

function setReconnectAnchor(a) {
  reconnectAnchor = a;
}

export { readingRef, keepReading, hideNewPill, offerPill, setReconnectAnchor, scheduleRestore };


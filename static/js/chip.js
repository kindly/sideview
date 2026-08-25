// The selection chip: the one creation gesture, plus the fixed corner
// buttons touch gets instead (the OS toolbar owns the floating airspace).
import { state, pageEditable, textOf } from './state.js';
import { $blocks, $bar } from './dom.js';
import { anchorOf, spotFrom } from './anchors.js';
import { startEdit } from './editor.js';
import { svc, vue } from './commentbar.js';

const $cbarToggle = document.createElement('button');
$cbarToggle.id = 'sv-cbar-toggle';
$cbarToggle.type = 'button';
$cbarToggle.title = 'comments';
document.body.appendChild($cbarToggle);
$cbarToggle.addEventListener('mousedown', (e) => e.preventDefault());
$cbarToggle.addEventListener('click', () => {
  // Tapping the chip collapses the selection before click lands (mobile),
  // so the draft comes from the remembered selection, not the live one.
  if (lastSel) {
    const held = lastSel;
    lastSel = null;
    startDraft(held.spot, held.text);
    return;
  }
  const open = !document.body.classList.contains('sv-cbar-open');
  document.body.classList.toggle('sv-cbar-open', open);
  localStorage.setItem('sv-cbar:' + state.selected, open ? 'open' : 'closed');
});

// ---- the selection chip: the one creation gesture -----------------------------
// Select any text in a block and a small "comment" chip appears; the selection
// becomes the quote, the containing element's text the context. Double-click
// works for free (it selects a word). No resting furniture in the content.

const BUBBLE_SVG = `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"
    fill="none" stroke="currentColor" stroke-width="1.8"
    stroke-linejoin="round" stroke-linecap="round">
  <path d="M21 14a3 3 0 0 1-3 3H8l-5 4V6a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3z"/>
  <path d="M7.5 8h9M7.5 12h5.5" stroke-width="1.6"/></svg>`;

// Touch screens never get the floating chip: the OS selection toolbar hovers
// over the selection with no API for where — competing for that airspace is
// unwinnable (author's report, 2026-08-08). The fixed corner chip takes the
// job instead, the one spot the native toolbar never covers.
const TOUCH = matchMedia('(hover: none)').matches;

// The chip grew a second verb (V4.sv, thread 63): comment and edit, a
// two-button strip, never a menu.
const PENCIL_SVG = `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
  stroke-linejoin="round"><path d="M17 3l4 4L8 20l-5 1 1-5z"/></svg>`;
const $chip = document.createElement('div');
$chip.id = 'sv-cchip';
$chip.hidden = true;
const $chipComment = document.createElement('button');
$chipComment.type = 'button';
$chipComment.className = 'sv-chip-btn';
$chipComment.setAttribute('aria-label', 'comment on the selection');
$chipComment.title = 'comment on the selection';
$chipComment.innerHTML = BUBBLE_SVG;
const $chipEdit = document.createElement('button');
$chipEdit.type = 'button';
$chipEdit.className = 'sv-chip-btn';
$chipEdit.setAttribute('aria-label', 'edit this block');
$chipEdit.title = 'edit this block';
$chipEdit.innerHTML = PENCIL_SVG;
$chip.append($chipComment, $chipEdit);
document.body.appendChild($chip);

// Touch gets edit too (round-11 drill, thread 77): a second fixed corner
// button while a selection is active — the same law the corner chip was
// born from: never compete with the OS selection toolbar's airspace.
// Visibility is pure CSS off body.sv-selecting; the lastSel grace window
// covers the tap collapsing the selection first.
const $ceditToggle = document.createElement('button');
$ceditToggle.id = 'sv-cedit-toggle';
$ceditToggle.type = 'button';
$ceditToggle.title = 'edit the selected block';
$ceditToggle.setAttribute('aria-label', 'edit the selected block');
$ceditToggle.innerHTML = PENCIL_SVG;
document.body.appendChild($ceditToggle);
$ceditToggle.addEventListener('mousedown', (e) => e.preventDefault());
$ceditToggle.addEventListener('click', () => {
  if (!lastSel) return;
  const held = lastSel;
  lastSel = null;
  document.body.classList.remove('sv-selecting');
  syncToggle();
  startEdit(held.spot, held.text);
});

let chipTimer = 0;
document.addEventListener('selectionchange', () => {
  clearTimeout(chipTimer);
  chipTimer = setTimeout(placeChip, 150);
});

// The selection, remembered: tapping any affordance collapses the live
// selection first on touch, so drafts read from here.
let lastSel = null;
let selClearTimer = 0;

// The one rule for the remembered selection's lifetime (rounds 12–13, the
// stuck-forever bug: two paths set the state and only one armed the clear).
// EVERY path that touches the state re-arms this: ~5s on touch after the
// last selection activity — long enough to read two corner buttons and
// choose, never stuck; 700ms on desktop, where the floating chip does the
// offering and only the in-flight click needs covering.
function armSelClear() {
  clearTimeout(selClearTimer);
  selClearTimer = setTimeout(() => {
    lastSel = null;
    document.body.classList.remove('sv-selecting');
    syncToggle();
  }, TOUCH ? 5000 : 700);
}

function placeChip() {
  // Typing or selecting inside an ask control is answering, not commenting.
  if (document.activeElement?.matches?.('.sv-ask textarea, .sv-ask input')) {
    $chip.hidden = true;
    return;
  }
  const sel = getSelection();
  let spot = null;
  if (sel && !sel.isCollapsed && sel.rangeCount) {
    const cont = sel.getRangeAt(0).commonAncestorContainer;
    if ((cont.nodeType === 1 ? cont : cont.parentElement)?.closest('#sv-blocks [data-block]')) {
      spot = spotFrom(sel.getRangeAt(0).startContainer);
    }
  }
  if (!spot) {
    $chip.hidden = true;
    armSelClear();
    return;
  }
  // A LIVE selection holds the state with no expiry (the user is mid-
  // gesture; desktop's chip click must find lastSel however long they
  // think). The expiry arms when the selection collapses (!spot above) or
  // when double-tap set the state without any selection event to follow.
  clearTimeout(selClearTimer);
  lastSel = { spot, text: sel.toString().trim() };
  if (TOUCH) {
    document.body.classList.add('sv-selecting');
    $ceditToggle.hidden = !pageEditable();
    $cbarToggle.classList.add('sv-sel');
    $cbarToggle.title = 'comment on the selection';
    return;
  }
  const range = sel.getRangeAt(0);
  const rects = range.getClientRects();
  const r = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
  $chipEdit.hidden = !pageEditable();
  $chip.style.top = scrollY + r.bottom + 8 + 'px';
  $chip.style.left = Math.min(scrollX + r.right + 4, scrollX + innerWidth - 92) + 'px';
  $chip.hidden = false;
}

// The corner chip is always the comment bubble (author, 2026-08-08) — the
// open-thread count rides as a small badge, and an active selection inks
// the border.
function syncToggle() {
  $cbarToggle.classList.remove('sv-sel');
  $cbarToggle.title = 'comments';
  $cbarToggle.innerHTML = BUBBLE_SVG;
  const openThreads = svc ? svc.threads.filter((t) => t.resolved_at == null) : [];
  if (openThreads.length) $cbarToggle.dataset.count = String(openThreads.length);
  else delete $cbarToggle.dataset.count;
  // Filled means the agent spoke last somewhere — the user's turn.
  const turn = openThreads.some((t) => {
    const cs = svc.comments.filter((c) => c.thread_id === t.id);
    return cs.length && cs[cs.length - 1].author === 'agent';
  });
  $cbarToggle.classList.toggle('sv-cbar-turn', turn);
}
syncToggle();

// Drafts are plural (author, 2026-08-09): a second gesture must never
// destroy an unfinished comment — it starts its own card, bound for its own
// thread. The one exception: the exact same spot refocuses the existing
// draft instead of duplicating it.
let draftSeq = 0;
function startDraft(spot, quote) {
  const block = spot?.closest('[data-block]');
  if (!block || !svc) return;
  const target = block.dataset.block;
  const anchor = anchorOf(spot);
  const existing = svc.drafts.find(
    (d) => d.page === state.selected && d.target === target && d.anchor === anchor
  );
  const key = existing ? existing.key : ++draftSeq;
  if (!existing) {
    svc.drafts.push({
      key,
      // The page is captured now, not at send: an unpinned tab can follow
      // activity elsewhere while the draft sits open, and a comment belongs
      // to the page it was written on (thread 28 was filed cross-page by
      // exactly that gap).
      page: state.selected,
      target,
      anchor,
      quote: quote.slice(0, 300),
      context: textOf(spot).trim().slice(0, 500),
      text: '',
      atts: [],
    });
  }
  getSelection()?.removeAllRanges();
  $chip.hidden = true;
  document.body.classList.add('sv-cbar-open');
  vue.nextTick(() => {
    // Drafts render at the top of the bar: bring the bar there, or a new
    // card is born out of view under a scrolled conversation (thread 87 —
    // a long-standing one).
    const scroll = $bar.querySelector('.sv-cbar-scroll');
    if (scroll) scroll.scrollTop = 0;
    $bar.querySelector(`textarea[data-draft="${key}"]`)?.focus({ preventScroll: true });
  });
}


$chip.addEventListener('mousedown', (e) => e.preventDefault()); // keep the selection
$chipComment.addEventListener('click', () => {
  if (!lastSel) return;
  const held = lastSel;
  lastSel = null;
  startDraft(held.spot, held.text);
});
$chipEdit.addEventListener('click', () => {
  if (!lastSel) return;
  const held = lastSel;
  lastSel = null;
  getSelection()?.removeAllRanges();
  $chip.hidden = true;
  startEdit(held.spot, held.text);
});

// Double-click produces a *selection*, and the chip offers the verbs —
// the unified gesture model (author, 2026-08-20, thread 63 on V4.sv). The
// straight-to-draft shortcut retired with it: one extra tap on commenting,
// accepted, in exchange for edit and comment sharing one entry.
$blocks.addEventListener('dblclick', (e) => {
  if (e.target.closest('a, button, input, textarea, iframe, #sv-comments')) return;
  const spot = spotFrom(e.target);
  if (!spot) return;
  const sel = getSelection();
  const selText = sel && !sel.isCollapsed ? sel.toString().trim() : '';
  lastSel = { spot, text: selText.length > 20 ? selText : textOf(spot).trim() };
  armSelClear();
  if (TOUCH) {
    document.body.classList.add('sv-selecting');
    $ceditToggle.hidden = !pageEditable();
    $cbarToggle.classList.add('sv-sel');
    $cbarToggle.title = 'comment on the selection';
    return;
  }
  $chipEdit.hidden = !pageEditable();
  $chip.style.top = scrollY + e.clientY + 12 + 'px';
  $chip.style.left = Math.min(scrollX + e.clientX + 4, scrollX + innerWidth - 92) + 'px';
  $chip.hidden = false;
});

export { BUBBLE_SVG, syncToggle, startDraft, $cbarToggle };


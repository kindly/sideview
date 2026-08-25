// Selection mechanics — and only mechanics. The chip's *rendering* folded
// into the comment island (V5.sv step 4d: one commenting UI); what stays
// here is the saga-hardened part: which element the gesture happened on,
// the remembered selection, and the one clear-timer every path re-arms
// (rounds 12–13 — the stuck-forever bug was two paths setting state and one
// arming the clear). This module pushes what the island should show through
// a sink and never touches the DOM beyond the body classes CSS keys off.
import { pageEditable, textOf } from './state.js';
import { $blocks } from './dom.js';
import { spotFrom } from './anchors.js';

// Touch screens never get the floating chip: the OS selection toolbar hovers
// over the selection with no API for where — competing for that airspace is
// unwinnable (author's report, 2026-08-08). The fixed corner buttons take
// the job instead, the one spot the native toolbar never covers.
export const TOUCH = matchMedia('(hover: none)').matches;

// The island registers here once Vue is up. Until then (or if Vue never
// loads) selections are tracked but nothing renders — commenting needs the
// island anyway.
const IDLE = { floating: null, pending: false, editable: false };
let sink = () => {};
export function onSelection(fn) {
  sink = fn;
}

// The selection, remembered: tapping any affordance collapses the live
// selection first on touch, so drafts read from here.
let lastSel = null;
let selClearTimer = 0;

// The one rule for the remembered selection's lifetime (rounds 12–13).
// EVERY path that touches the state re-arms this: ~5s on touch after the
// last selection activity — long enough to read two corner buttons and
// choose, never stuck; 700ms on desktop, where the floating chip does the
// offering and only the in-flight click needs covering.
function armSelClear() {
  clearTimeout(selClearTimer);
  selClearTimer = setTimeout(() => {
    lastSel = null;
    document.body.classList.remove('sv-selecting');
    sink(IDLE);
  }, TOUCH ? 5000 : 700);
}

// An affordance consumed the selection: hand it over and go idle.
export function takeSelection() {
  const held = lastSel;
  lastSel = null;
  clearTimeout(selClearTimer);
  document.body.classList.remove('sv-selecting');
  sink(IDLE);
  return held;
}

let chipTimer = 0;
document.addEventListener('selectionchange', () => {
  clearTimeout(chipTimer);
  chipTimer = setTimeout(placeChip, 150);
});

function offer(spot, text, at) {
  // A LIVE selection holds the state with no expiry (the user is mid-
  // gesture; desktop's chip click must find lastSel however long they
  // think). The expiry arms when the selection collapses or when double-tap
  // set the state without any selection event to follow.
  lastSel = { spot, text };
  const editable = pageEditable();
  if (TOUCH) {
    document.body.classList.add('sv-selecting');
    sink({ floating: null, pending: true, editable });
    return;
  }
  sink({
    floating: {
      top: scrollY + at.bottom + 8,
      left: Math.min(scrollX + at.right + 4, scrollX + innerWidth - 92),
    },
    pending: true,
    editable,
  });
}

function placeChip() {
  // Typing or selecting inside an ask control is answering, not commenting.
  if (document.activeElement?.matches?.('.sv-ask textarea, .sv-ask input')) {
    sink(IDLE);
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
    // The floating chip goes at once; on touch the corner buttons ride the
    // body class until the grace timer clears it — the tap that collapses
    // the selection must still find its target.
    sink(document.body.classList.contains('sv-selecting')
      ? { floating: null, pending: true, editable: pageEditable() }
      : IDLE);
    armSelClear();
    return;
  }
  clearTimeout(selClearTimer);
  const range = sel.getRangeAt(0);
  const rects = range.getClientRects();
  const r = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
  offer(spot, sel.toString().trim(), r);
}

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
  offer(spot, selText.length > 20 ? selText : textOf(spot).trim(), {
    bottom: e.clientY + 4,
    right: e.clientX,
  });
  armSelClear();
});

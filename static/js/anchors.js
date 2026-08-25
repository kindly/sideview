// Anchors: text positions that survive edits (h: heading ids, p: content
// hashes), and the element a gesture happened on.
import { blockEl } from './dom.js';
import { textOf } from './state.js';

// FNV-1a 64 over the whitespace-normalized text, low 48 bits as 12 hex —
// the `p:` anchor. Vector: anchorHash('the quick brown fox') = '8115ea47e2c8'.
// The daemon-side twin arrives with re-resolution (V2.sv).
function anchorHash(text) {
  const s = text.replace(/\s+/g, ' ').trim();
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(s)) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return (h & 0xffffffffffffn).toString(16).padStart(12, '0');
}

// thread -> the element its anchor names right now, or null (orphaned —
// which on a plan usually means the feedback was addressed and the text
// changed; the bar says so rather than mourning it).
function resolveAnchor(t) {
  const block = blockEl(t.target);
  if (!block) return null;
  if (!t.anchor) return block;
  if (t.anchor.startsWith('h:')) {
    const el = document.getElementById(t.anchor.slice(2));
    return el && block.contains(el) ? el : null;
  }
  if (t.anchor.startsWith('p:')) {
    const want = t.anchor.slice(2);
    for (const p of block.querySelectorAll('p, li, pre')) {
      if (anchorHash(textOf(p)) === want) return p;
    }
    return null;
  }
  return null; // l: — per-line diff placement lands with watched diffs
}

// element -> the anchor string a new thread there would carry.
function anchorOf(el) {
  const block = el.closest('[data-block]');
  if (!block || el === block) return '';
  if (/^H[1-6]$/.test(el.tagName) && el.id) return 'h:' + el.id;
  if (['P', 'LI', 'PRE'].includes(el.tagName)) return 'p:' + anchorHash(textOf(el));
  return '';
}

function spotFrom(node) {
  const el = node instanceof Element ? node : node?.parentElement;
  return (
    el?.closest(':is(h1, h2, h3, h4, h5, h6)[id], p, li, pre') ||
    el?.closest('#sv-blocks [data-block]')
  );
}

export { anchorHash, resolveAnchor, anchorOf, spotFrom };


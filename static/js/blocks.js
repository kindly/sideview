// Block lifecycle: parse the daemon's HTML, place by ord, keep the reading
// still. Plus the srcdoc envelope, diff view prefs, ext frames, csv freeze.
import { state, textOf } from './state.js';
import { $blocks, blockEl } from './dom.js';
import { keepReading, offerPill } from './scroll.js';
import { hydrateAsk } from './ask.js';
import { pendingBlockEv } from './editor.js';
import { startDraft, BUBBLE_SVG } from './chip.js';
import { Idiomorph } from '/assets/vendor/idiomorph.esm.js';

// Morph config: runtime decoration lives in inline styles (the envelope's
// iframe height, csv freeze offsets) that the server's fresh HTML never
// carries — a morph must not strip them just because the new node is bare.
const MORPH_OPTS = {
  callbacks: {
    beforeAttributeUpdated: (attr, node, mutationType) => {
      if (attr === 'style' && mutationType === 'remove') return false;
    },
  },
};

// ---- the iframe envelope --------------------------------------------------
// html blocks are sandboxed srcdoc iframes; the envelope is their one channel
// ({sv: 1, type: …}). Size flows out — the iframe grows to its content and
// the 85vh placeholder retires — and theme flows in, fixing the known gap
// where srcdoc themed off the OS instead of the viewer's override. The first
// size report doubles as the handshake that triggers the theme send.

addEventListener('message', (e) => {
  const m = e.data;
  if (!m || m.sv !== 1) return;
  for (const f of document.querySelectorAll('iframe.sv-html')) {
    if (f.contentWindow !== e.source) continue;
    if (m.type === 'size' && !f.dataset.svFixed && Number.isFinite(m.height)) {
      const px = Math.ceil(m.height) + 'px';
      // Compensated: an iframe growing above the viewport must not shove
      // the reading. Remembered: the next rebuild starts at this size.
      keepReading(() => { f.style.height = px; });
      const b = f.closest('[data-block]');
      if (b) localStorage.setItem(iframeKey(b.dataset.block), String(Math.ceil(m.height)));
    }
    f.contentWindow.postMessage(
      { sv: 1, type: 'theme', mode: document.documentElement.getAttribute('data-bs-theme') },
      '*'
    );
    break;
  }
});

// ---- diff blocks --------------------------------------------------------------
// The agent's view attribute is the default; the viewer's toggle wins and is
// remembered per block — the same symmetry as the outline rail. Delegated,
// because blocks are replaced wholesale on every SSE patch.

function diffPrefKey(block) {
  return 'sv-diffview:' + state.selected + ':' + block;
}

function applyDiffPref(el) {
  const fig = el.querySelector('.sv-diff');
  if (!fig) return;
  const stored = localStorage.getItem(diffPrefKey(el.dataset.block));
  if (stored === 'split' || stored === 'unified') fig.dataset.view = stored;
}

$blocks.addEventListener('click', (e) => {
  const t = e.target.closest('.sv-diff-toggle');
  if (!t) return;
  const fig = t.closest('.sv-diff');
  const next = fig.dataset.view === 'split' ? 'unified' : 'split';
  fig.dataset.view = next;
  const section = t.closest('[data-block]');
  if (section) localStorage.setItem(diffPrefKey(section.dataset.block), next);
});

// ---- blocks -----------------------------------------------------------------

function elementFor(blockId, html, ord) {
  const tpl = document.createElement('template');
  tpl.innerHTML = html;
  const el = tpl.content.firstElementChild;
  if (!el) return null;
  el.dataset.ord = ord;
  addBlockComment(el);
  return el;
}

// A thread on the block *as a whole* (author, 2026-08-13). Until now every
// gesture went through a text selection, so a block with no selectable text —
// an iframe today, a framed table tomorrow — could not be commented on at
// all. It is also the simplest kind of thread there is: the empty anchor
// means the block's tail, so it never orphans; it outlives every edit to the
// block's content and dies only with the block.
function addBlockComment(el) {
  if (!el.matches?.('[data-block]')) return;
  // Only blocks with no selectable text get one (author, 2026-08-13): an
  // iframe, a lone image. Everywhere else double-click and selection already
  // work, and a second affordance would make the user reason about a
  // block-type divide they should never have to see.
  if (textOf(el).trim()) return;
  const b = document.createElement('button');
  b.type = 'button';
  b.className = 'sv-block-comment';
  b.title = 'comment on this block';
  b.setAttribute('aria-label', 'comment on this block');
  b.innerHTML = BUBBLE_SVG;
  b.addEventListener('click', () => startDraft(el, ''));
  el.appendChild(b);
}

// Scripts parsed via innerHTML are inert; markup blocks are deliberately
// unsanitized (see V0.md), so re-create them to let them run.
// Extension frames are same-origin (EXTENSIONS.md), so no envelope: the
// parent measures the document directly and follows it — unless the block
// pinned a height, which wins.
function wireExtFrames(el) {
  for (const frame of el.querySelectorAll('iframe.sv-ext')) {
    if (frame.dataset.svFixed) continue;
    if (frame.dataset.svWired) continue; // morph keeps nodes: wire once
    frame.dataset.svWired = '1';
    frame.addEventListener('load', () => {
      try {
        const doc = frame.contentDocument;
        const set = () => {
          const h = Math.max(
            doc.documentElement.scrollHeight,
            doc.body ? doc.body.scrollHeight : 0
          );
          if (h > 0) frame.style.height = h + 'px';
        };
        set();
        new ResizeObserver(set).observe(doc.documentElement);
      } catch (e) {
        /* a foreign frame: leave the default height */
      }
    });
  }
}

// Frozen csv columns: CSS owns the stickiness, this pass only supplies the
// measured left offsets (column widths are unknowable before layout).
// Positions, never renders — the sv-csv line.
function wireCsvFreeze(el) {
  for (const fig of el.querySelectorAll('figure.sv-csv[data-sv-freeze]')) {
    const n = parseInt(fig.dataset.svFreeze, 10) || 0;
    const set = () => {
      const ths = fig.querySelectorAll('thead th');
      let left = 0;
      for (let i = 0; i < n && i < ths.length; i++) {
        fig.style.setProperty('--sv-fz-' + i, left + 'px');
        left += ths[i].getBoundingClientRect().width;
      }
    };
    set();
    // Fonts shift widths; measure once more when they settle.
    document.fonts?.ready.then(set);
  }
}

function activateScripts(el) {
  for (const old of el.querySelectorAll('script')) {
    const s = document.createElement('script');
    for (const a of old.attributes) s.setAttribute(a.name, a.value);
    s.textContent = old.textContent;
    old.replaceWith(s);
  }
}

function applyBlock(ev) {
  // Genuinely live, as opposed to the replay burst after every (re)connect.
  const live = Date.now() - state.connectedAt > 1500;
  const existing = blockEl(ev.block);
  // Never yank a block out from under its open editor: hold the patch and
  // apply it when the editor closes (the from-hash guard keeps saves honest
  // meanwhile).
  if (existing && existing.querySelector('.sv-editor')) {
    pendingBlockEv.set(ev.block, ev);
    return;
  }
  if (ev.action === 'remove') {
    if (existing) keepReading(() => existing.remove());
    return;
  }
  const el = elementFor(ev.block, ev.html, ev.ord);
  if (!el) return;
  applyDiffPref(el);
  hydrateAsk(el);
  applyIframeMemory(el, ev.block);
  if (existing && (existing.dataset.ord || '') === ev.ord) {
    // update in place. Morph (idiomorph, V5.sv thread 121): unchanged nodes
    // keep their scroll, selection, open details and iframe state, so most
    // patches move nothing and the reading anchor has nothing to do. Blocks
    // carrying scripts (Vue islands) keep the wholesale path — a morphed
    // script element doesn't re-execute, and an island's own state is the
    // iframe/DOM it built, which replaceWith + activateScripts handles.
    if (el.querySelector('script') || existing.querySelector('script')) {
      keepReading(() => existing.replaceWith(el));
      activateScripts(el);
    } else {
      keepReading(() => Idiomorph.morph(existing, el, MORPH_OPTS));
      // Runtime state the morph reset re-applies on the surviving node.
      applyDiffPref(existing);
      hydrateAsk(existing);
      wireExtFrames(existing);
      wireCsvFreeze(existing);
    }
    return;
  }
  keepReading(() => {
    // The block moved (file order is the order): re-place it below.
    if (existing) existing.remove();
    // Place before the first sibling at-or-after this ord. >= and not >:
    // during an insertion cascade the not-yet-updated sibling below carries
    // the SAME stale ord, and strict > placed blocks one slot too low —
    // transient teleports the reading anchor then chased downward,
    // compounding to the end of the page (found live, 2026-08-08; deletions
    // never tie, which is why only insertions broke).
    const next = [...$blocks.children].find((c) => (c.dataset.ord || '') >= ev.ord);
    if (next) $blocks.insertBefore(el, next);
    else $blocks.appendChild(el);
  });
  if (live) el.classList.add('sv-arrive');
  activateScripts(el);
  wireExtFrames(el);
  wireCsvFreeze(el);
  // Never scroll for new content (the author's rule); when it lands out of
  // view, offer the pill instead.
  if (live && el.getBoundingClientRect().top > innerHeight) offerPill(el);
}

// Iframes are reborn on every replay; remembering their last reported size
// means the envelope's first report confirms the layout instead of shoving it.
function iframeKey(block) {
  return 'sv-iframeh:' + state.selected + ':' + block;
}
function applyIframeMemory(el, block) {
  const h = localStorage.getItem(iframeKey(block));
  if (!h) return;
  for (const f of el.querySelectorAll('iframe.sv-html')) {
    if (!f.dataset.svFixed) f.style.height = h + 'px';
  }
}

export { applyBlock };


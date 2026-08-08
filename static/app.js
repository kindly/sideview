// The whole client. The daemon sends rendered HTML plus each block's declared
// headings; the job here is: hold an EventSource, place/replace/remove elements
// by block id, build the contents rail from declarations, and track where the
// reader is. The rail has two coherent modes, chosen per session by the agent
// (`session set --outline`): scrollspy (default — the page is always the whole
// document, the rail follows the scroll) and tabs (sections are separate
// panes — the mode for prototypes and app-like pages).
'use strict';

// Bumped by hand whenever client behaviour changes: the daemon's version
// skew warns loudly, but a stale tab's JS is invisible — this stamp (console
// + the brand tooltip) is how you tell which client a tab is running.
const CLIENT_STAMP = '2026-08-08l bar-toggle';
console.log('sideview client', CLIENT_STAMP);

const state = {
  sessions: [],          // [{id, last_active_at, props}] most recent first
  blocks: new Map(),     // session id -> Map(block id -> {ord, html, headings})
  selected: null,        // session id
  pinned: false,         // the user deliberately clicked a session
  section: null,         // tabs mode: the selected section key
  spyActive: null,       // scrollspy mode: the section currently in view
  expand: new Map(),     // section key -> bool, manual twist overrides (per session)
  connectedAt: 0,        // when the stream last opened; gates the arrival ink
  conversations: new Map(), // page id -> {threads: [], comments: []} from SSE
};

let outline = { sections: [], blockSections: new Map() };
let railRefs = new Map(); // section key -> {link, twist, kids}

const $blocks = document.getElementById('sv-blocks');
const $sessions = document.getElementById('sv-sessions');
const $status = document.getElementById('sv-status');
const $brand = document.getElementById('sv-brand');
const $outline = document.getElementById('sv-outline');
const $railToggle = document.getElementById('sv-rail-toggle');
const $outlineList = document.getElementById('sv-outline-list');

// /s/<session> pins that session; / follows the most recently active one.
const pathMatch = location.pathname.match(/^\/s\/(.+)$/);
if (pathMatch) {
  state.selected = decodeURIComponent(pathMatch[1]);
  state.pinned = true;
}

function sessionProps() {
  const s = state.sessions.find((x) => x.id === state.selected);
  return (s && s.props) || {};
}

// The agent's declared mode: tabs, or scrollspy (the default). `off` means
// scrollspy with the rail starting collapsed.
function railMode() {
  return sessionProps().outline === 'tabs' ? 'tabs' : 'scrollspy';
}

// Whether the rail is open: the viewer's own fold/unfold (remembered per
// page) wins over the agent's `--outline off`.
function railOpen() {
  const stored = localStorage.getItem('sv-outline:' + state.selected);
  if (stored === 'on' || stored === 'off') return stored === 'on';
  return sessionProps().outline !== 'off';
}

$railToggle.addEventListener('click', () => {
  localStorage.setItem('sv-outline:' + state.selected, railOpen() ? 'off' : 'on');
  refreshOutline();
});


// The theme override: OS is the default, the viewer's choice wins and is
// remembered — cycling auto → light → dark. svTheme() lives in index.html's
// pre-paint script. One known gap: iframe-isolated html blocks theme off the
// OS, not the override (a sandboxed srcdoc can't see our localStorage).
const $theme = document.getElementById('sv-theme');
function themeState() {
  const pref = localStorage.getItem('sv-theme');
  return pref === 'light' || pref === 'dark' ? pref : 'auto';
}
function renderThemeButton() {
  const s = themeState();
  $theme.textContent = s === 'auto' ? '◐' : s === 'light' ? '☀' : '☾';
  $theme.title = 'theme: ' + (s === 'auto' ? 'following the system' : s) + ' — click to change';
}
$theme.addEventListener('click', () => {
  const next = { auto: 'light', light: 'dark', dark: null }[themeState()];
  if (next) localStorage.setItem('sv-theme', next);
  else localStorage.removeItem('sv-theme');
  svTheme();
  renderThemeButton();
  // Class-based highlighting and CSS variables re-theme by stylesheet alone;
  // only the sandboxed iframes need telling, over the envelope.
  const mode = document.documentElement.getAttribute('data-bs-theme');
  for (const f of document.querySelectorAll('iframe.sv-html')) {
    f.contentWindow?.postMessage({ sv: 1, type: 'theme', mode }, '*');
  }
});
renderThemeButton();


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

const es = new EventSource('/events');
es.addEventListener('open', () => {
  state.connectedAt = Date.now();
  const ref = readingRef();
  if (ref) {
    reconnectAnchor = { block: ref.block, top: ref.top, until: Date.now() + 8000 };
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
  document.body.classList.remove('sv-disconnected');
  $status.hidden = true;
  $brand.title = 'connected · client ' + CLIENT_STAMP;
});
es.addEventListener('error', () => {
  // EventSource reconnects on its own; the dot goes hollow while it does.
  document.body.classList.add('sv-disconnected');
  $status.hidden = false;
  $brand.title = 'reconnecting';
});

es.addEventListener('sessions', (e) => {
  state.sessions = JSON.parse(e.data).sessions;
  // The snapshot is authoritative: a session it doesn't list is gone, blocks
  // and all — this is how deletion reaches every open tab.
  const ids = new Set(state.sessions.map((s) => s.id));
  for (const held of [...state.blocks.keys()]) {
    if (!ids.has(held)) state.blocks.delete(held);
  }
  // Chips sit in stable creation order; "which page should an unpinned tab
  // show" is decided by activity instead.
  const mostActive = state.sessions.reduce(
    (a, s) => (!a || s.last_active_at > a.last_active_at ? s : a),
    null
  );
  if (state.selected && !ids.has(state.selected)) {
    state.pinned = false;
    history.replaceState(null, '', '/');
    switchSession(mostActive?.id ?? null);
  } else if (!state.pinned && mostActive && mostActive.id !== state.selected) {
    switchSession(mostActive.id);
  }
  renderSessionStrip();
  refreshOutline(); // a session's outline property may have changed
});

es.addEventListener('block', (e) => {
  const ev = JSON.parse(e.data);
  let per = state.blocks.get(ev.session);
  if (!per) { per = new Map(); state.blocks.set(ev.session, per); }
  if (ev.action === 'remove') per.delete(ev.block);
  else per.set(ev.block, { ord: ev.ord, html: ev.html, headings: ev.headings || [] });
  if (ev.session === state.selected) {
    applyBlock(ev);
    refreshOutline();
    scheduleConversation(); // a replaced block sheds its count-dots
    scheduleRestore();      // reconnect replay: put the reading back
  }
});

es.addEventListener('threads', (e) => {
  const ev = JSON.parse(e.data);
  state.conversations.set(ev.page, { threads: ev.threads, comments: ev.comments });
  if (ev.page === state.selected) scheduleConversation();
});

function switchSession(id) {
  state.selected = id;
  state.section = null;
  state.spyActive = null;
  state.expand.clear();
  renderAllBlocks();
}

function renderSessionStrip() {
  $sessions.textContent = '';
  for (const s of state.sessions) {
    const chip = document.createElement('span');
    chip.className = 'sv-chip' + (s.id === state.selected ? ' active' : '');

    const btn = document.createElement('button');
    btn.className = 'sv-chip-label';
    btn.textContent = (s.props && s.props.label) || shortLabel(s.id);
    btn.title = s.id;
    btn.addEventListener('click', () => {
      state.pinned = true;
      history.pushState(null, '', '/s/' + encodeURIComponent(s.id));
      switchSession(s.id);
      renderSessionStrip();
    });

    // Deletion is the page's one write, and it's irrevocable (the file goes),
    // so it's two-step: first click arms, second confirms, and it disarms
    // itself. No dialog — dialogs train reflexive clicking.
    const del = document.createElement('button');
    del.className = 'sv-chip-del';
    del.textContent = '×';
    del.title = 'delete this page';
    let disarm = 0;
    del.addEventListener('click', () => {
      if (!chip.classList.contains('sv-armed')) {
        chip.classList.add('sv-armed');
        del.title = 'click again to delete — removes the page file';
        disarm = setTimeout(() => {
          chip.classList.remove('sv-armed');
          del.title = 'delete this page';
        }, 3000);
        return;
      }
      clearTimeout(disarm);
      fetch('/api/sessions/' + encodeURIComponent(s.id), { method: 'DELETE' }).catch(() => {});
    });

    chip.append(btn, del);
    $sessions.appendChild(chip);
  }
}

function shortLabel(id) {
  return id.length > 12 ? id.slice(0, 8) + '…' : id;
}

// ---- the contents rail --------------------------------------------------------
// Sections are blocks that declare an h1/h2; deeper headings nest under the
// section in force, and a headingless block belongs to the section it follows.
// Blocks before any section are front matter, visible on every tab.

function computeOutline() {
  // An explicit outline (sideview outline → outline_spec prop) is used
  // verbatim: the agent's ordered list, inference off. Prose derivation
  // below stays the default.
  const spec = sessionProps().outline_spec;
  if (Array.isArray(spec) && spec.length) {
    const anchorId = (a) => (typeof a === 'string' && a.startsWith('h:') ? a.slice(2) : null);
    return {
      sections: spec.map((e, i) => ({
        key: 'spec/' + i,
        block: null,
        title: String(e.title || ''),
        id: anchorId(e.anchor),
        children: (e.children || []).map((c) => ({
          text: String(c.title || ''),
          id: anchorId(c.anchor),
          block: null,
        })),
      })),
      blockSections: new Map(),
    };
  }

  const sections = [];
  const blockSections = new Map(); // block id -> Set(section index); empty = front matter
  const per = state.blocks.get(state.selected);
  if (!per) return { sections, blockSections };
  const ordered = [...per.entries()].sort((a, b) => (a[1].ord < b[1].ord ? -1 : 1));
  let current = -1;
  for (const [id, b] of ordered) {
    const memberOf = new Set();
    for (const h of b.headings || []) {
      if (h.level <= 2) {
        // The heading's own anchor, when it has one: a block can declare
        // many sections (a multi-file diff, a prose block with several ##s),
        // and both the spy and the rail clicks must resolve to the heading,
        // not the shared block top.
        sections.push({ key: id + '/' + sections.length, block: id, title: h.text, id: h.id, children: [] });
        current = sections.length - 1;
        memberOf.add(current);
      } else if (current >= 0) {
        sections[current].children.push({ text: h.text, id: h.id, block: id });
        memberOf.add(current);
      }
    }
    if (memberOf.size === 0 && current >= 0) memberOf.add(current);
    blockSections.set(id, memberOf);
  }
  return { sections, blockSections };
}

function refreshOutline() {
  outline = computeOutline();
  const mode = railMode();
  const hasRail = outline.sections.length > 1;

  if (mode === 'tabs') {
    if (!outline.sections.some((s) => s.key === state.section)) {
      state.section = outline.sections[0]?.key ?? null;
    }
  }

  $outlineList.textContent = '';
  railRefs = new Map();

  const link = (label, onclick) => {
    const el = document.createElement('button');
    el.type = 'button';
    el.className = 'sv-o-link';
    el.textContent = label;
    el.title = label;
    el.addEventListener('click', onclick);
    return el;
  };

  for (const s of outline.sections) {
    const row = document.createElement('div');
    row.className = 'sv-o-row';
    const refs = { link: null, twist: null, kids: null };

    if (s.children.length) {
      const twist = document.createElement('button');
      twist.type = 'button';
      twist.className = 'sv-o-twist';
      twist.setAttribute('aria-label', 'toggle subsections');
      twist.addEventListener('click', () => {
        state.expand.set(s.key, !isExpanded(s.key));
        styleRail();
      });
      row.appendChild(twist);
      refs.twist = twist;
    } else {
      row.appendChild(
        Object.assign(document.createElement('span'), { className: 'sv-o-spacer' })
      );
    }

    refs.link = link(s.title, () => goToSection(s));
    row.appendChild(refs.link);
    $outlineList.appendChild(row);

    if (s.children.length) {
      const kids = document.createElement('div');
      kids.className = 'sv-o-kids';
      const kidsInner = document.createElement('div');
      kidsInner.className = 'sv-o-kids-inner';
      for (const c of s.children) {
        kidsInner.appendChild(link(c.text, () => goToChild(s, c)));
      }
      kids.appendChild(kidsInner);
      $outlineList.appendChild(kids);
      refs.kids = kids;
    }

    railRefs.set(s.key, refs);
  }

  document.body.classList.toggle('sv-rail', hasRail);
  const open = railOpen();
  document.body.classList.toggle('sv-rail-collapsed', hasRail && !open);
  $outline.classList.toggle('collapsed', !open);
  $railToggle.setAttribute('aria-expanded', String(open));
  $railToggle.setAttribute('aria-label', open ? 'collapse contents' : 'expand contents');

  applyVisibility();
  if (mode === 'scrollspy') updateSpy(true);
  else styleRail();
}

function activeKey() {
  return railMode() === 'tabs' ? state.section : state.spyActive;
}

function isExpanded(key) {
  return state.expand.has(key) ? state.expand.get(key) : key === activeKey();
}

// Restyle active/expanded without rebuilding — cheap enough for scroll events.
function styleRail() {
  const active = activeKey();
  for (const [key, refs] of railRefs) {
    refs.link.classList.toggle('active', key === active);
    const expanded = isExpanded(key);
    if (refs.twist) refs.twist.setAttribute('aria-expanded', String(expanded));
    if (refs.kids) refs.kids.classList.toggle('open', expanded);
  }
}

function sectionEl(s) {
  return (s.id && document.getElementById(s.id)) || blockEl(s.block);
}

function goToSection(s) {
  if (railMode() === 'tabs') {
    state.section = s.key;
    applyVisibility();
    styleRail();
    scrollTo({ top: 0 });
  } else {
    sectionEl(s)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }
}

function goToChild(s, c) {
  if (railMode() === 'tabs' && state.section !== s.key) {
    state.section = s.key;
    applyVisibility();
    styleRail();
  }
  const target = (c.id && document.getElementById(c.id)) || blockEl(c.block);
  target?.scrollIntoView({ block: 'start', behavior: 'smooth' });
}

function blockEl(id) {
  return id ? $blocks.querySelector(`[data-block="${CSS.escape(id)}"]`) : null;
}

function applyVisibility() {
  const tabs = railMode() === 'tabs' && outline.sections.length > 1;
  const selIdx = outline.sections.findIndex((s) => s.key === state.section);
  for (const el of $blocks.children) {
    if (!tabs) {
      el.style.display = '';
      continue;
    }
    const memberOf = outline.blockSections.get(el.dataset.block);
    const visible = !memberOf || memberOf.size === 0 || memberOf.has(selIdx);
    el.style.display = visible ? '' : 'none';
  }
}

// The spy: the active section is the last one whose first block has scrolled
// up to (or past) the reading line just below the header.
function updateSpy(force) {
  if (railMode() !== 'scrollspy' || outline.sections.length < 2) return;
  const readingLine = 90;
  let active = outline.sections[0].key;
  for (const s of outline.sections) {
    const el = sectionEl(s);
    if (el && el.getBoundingClientRect().top <= readingLine) active = s.key;
    else break;
  }
  if (force || active !== state.spyActive) {
    state.spyActive = active;
    styleRail();
  }
}

let spyScheduled = false;
addEventListener('scroll', () => {
  if (spyScheduled) return;
  spyScheduled = true;
  requestAnimationFrame(() => {
    spyScheduled = false;
    updateSpy(false);
  });
}, { passive: true });

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
  return el;
}

// Scripts parsed via innerHTML are inert; markup blocks are deliberately
// unsanitized (see V0.md), so re-create them to let them run.
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
  if (ev.action === 'remove') {
    if (existing) keepReading(() => existing.remove());
    return;
  }
  const el = elementFor(ev.block, ev.html, ev.ord);
  if (!el) return;
  applyDiffPref(el);
  applyIframeMemory(el, ev.block);
  if (existing && (existing.dataset.ord || '') === ev.ord) {
    // update: patch in place, compensated so the reading doesn't move
    keepReading(() => existing.replaceWith(el));
    activateScripts(el);
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
  // Never scroll for new content (the author's rule); when it lands out of
  // view, offer the pill instead.
  if (live && !newBelowEl && el.getBoundingClientRect().top > innerHeight) {
    newBelowEl = el;
    $newpill.hidden = false;
  }
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

function renderAllBlocks() {
  $blocks.textContent = '';
  const per = state.blocks.get(state.selected);
  if (per) {
    const sorted = [...per.entries()].sort((a, b) => (a[1].ord < b[1].ord ? -1 : 1));
    for (const [id, b] of sorted) {
      const el = elementFor(id, b.html, b.ord);
      if (el) {
        applyDiffPref(el);
        $blocks.appendChild(el);
        activateScripts(el);
      }
    }
  }
  refreshOutline();
  scheduleConversation();
}

// ---- conversation: the comment bar -------------------------------------------
// The author's redesign after real use (2026-08-08): a comment on a plan is
// usually a change-request for the very text it anchors to, so success
// destroys the anchor — anchored inline display optimized for the rare
// thread. Conversations now live in one stable place: a right bar, the
// first Vue island (the adoption point HANDOFF recorded). Inline there is
// no resting furniture at all — select text and a chip appears; the
// selection is the quote, the containing element's text the context.

const $bar = document.getElementById('sv-comments');
let vue = null;   // the vendored ESM module, once loaded
let svc = null;   // reactive conversation store, once mounted

// Small screens: the bar is an overlay, so it needs a way in and a way out —
// a toggle chip (bottom-right, showing the open-thread count), a tap on the
// content to dismiss, and auto-open when a draft begins.
const $cbarToggle = document.createElement('button');
$cbarToggle.id = 'sv-cbar-toggle';
$cbarToggle.type = 'button';
$cbarToggle.title = 'comments';
document.body.appendChild($cbarToggle);
$cbarToggle.addEventListener('click', () => {
  document.body.classList.toggle('sv-cbar-open');
});
document.addEventListener('click', (e) => {
  if (!document.body.classList.contains('sv-cbar-open')) return;
  if (e.target.closest('#sv-comments, #sv-cbar-toggle, #sv-cchip')) return;
  document.body.classList.remove('sv-cbar-open');
});

import('/assets/vendor/vue.esm-browser.prod.js')
  .then((m) => { vue = m; mountCommentBar(); syncConversation(); })
  .catch((e) => console.warn('sideview: comment bar disabled (vue failed to load)', e));

function conversation() {
  return state.conversations.get(state.selected) || { threads: [], comments: [] };
}

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

// Nothing of ours lives inside block content any more (the bar is outside,
// the chip is body-level) — and it must stay that way, or content hashes
// would see decoration.
function textOf(el) {
  return el.textContent;
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

function syncConversation() {
  if (!svc) return;
  const conv = conversation();
  svc.page = state.selected;
  svc.threads = conv.threads;
  svc.comments = conv.comments;
  const attach = {};
  for (const t of conv.threads) attach[t.id] = !!resolveAnchor(t);
  svc.attach = attach;
  const open = conv.threads.filter((t) => t.resolved_at == null).length;
  $cbarToggle.textContent = open ? String(open) : '·';
}

async function postComment(payload) {
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

// ---- the bar itself: the first Vue island ------------------------------------

function mountCommentBar() {
  const { createApp, reactive, computed, watchEffect, nextTick } = vue;
  svc = reactive({ page: null, threads: [], comments: [], attach: {}, draft: null });

  // Layout classes live on body so the CSS grid can breathe around the bar.
  watchEffect(() => {
    const has = svc.threads.length > 0 || !!svc.draft;
    document.body.classList.toggle('sv-cbar', has);
  });

  createApp({
    setup() {
      const open = computed(() => svc.threads.filter((t) => t.resolved_at == null));
      const resolved = computed(() => svc.threads.filter((t) => t.resolved_at != null));
      const collapsed = reactive({});
      const replies = reactive({});
      const error = reactive({ msg: '' });

      const commentsFor = (id) => svc.comments.filter((c) => c.thread_id === id);
      const lastAuthor = (id) => {
        const cs = commentsFor(id);
        return cs.length ? (cs[cs.length - 1].author || 'user') : null;
      };
      const fmt = (ts) =>
        new Date(ts).toLocaleString(undefined, {
          month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
        });

      const jump = (t) => {
        const el = resolveAnchor(t);
        if (!el) return;
        el.scrollIntoView({ block: 'center', behavior: 'smooth' });
        el.classList.remove('sv-flash');
        void el.offsetWidth; // restart the animation
        el.classList.add('sv-flash');
      };
      const send = async (payload, after) => {
        error.msg = '';
        try { await postComment(payload); after && after(); }
        catch (e) { error.msg = String(e.message || e); }
      };
      const reply = (t) => {
        const body = (replies[t.id] || '').trim();
        if (!body) return;
        send({ thread: t.id, body }, () => { replies[t.id] = ''; });
      };
      const sendDraft = () => {
        const d = svc.draft;
        if (!d || !d.text.trim()) return;
        send(
          { page: svc.page, target: d.target, anchor: d.anchor,
            quote: d.quote || null, context: d.context || null, body: d.text.trim() },
          () => { svc.draft = null; }
        );
      };

      return {
        svc, open, resolved, collapsed, replies, error,
        commentsFor, lastAuthor, fmt, jump, reply, sendDraft,
        attach: computed(() => svc.attach),
        resolve: (t) => setResolution(t.id, false),
        reopen: (t) => setResolution(t.id, true),
        toggle: (id) => { collapsed[id] = !collapsed[id]; },
        cancelDraft: () => { svc.draft = null; },
      };
    },
    template: `
      <div v-if="svc.threads.length || svc.draft" class="sv-cbar-inner">
        <div class="sv-cbar-title">Comments</div>
        <div v-if="error.msg" class="sv-cbar-error">{{ error.msg }}</div>

        <div v-if="svc.draft" class="sv-cbar-card sv-cbar-draft">
          <blockquote v-if="svc.draft.quote">{{ svc.draft.quote }}</blockquote>
          <textarea v-model="svc.draft.text" rows="3" placeholder="Comment…"
                    @keydown.meta.enter="sendDraft" @keydown.ctrl.enter="sendDraft"
                    @keydown.esc="cancelDraft"></textarea>
          <div class="sv-cbar-actions">
            <button type="button" @click="sendDraft">comment</button>
            <button type="button" class="sv-quiet" @click="cancelDraft">cancel</button>
          </div>
        </div>

        <div v-for="t in open" :key="t.id"
             class="sv-cbar-card"
             :class="{ 'sv-turn': lastAuthor(t.id) === 'agent' }">
          <div class="sv-cbar-meta">
            <button v-if="attach[t.id]" type="button" class="sv-jump"
                    title="jump to the spot" @click="jump(t)">↩ {{ t.target }}</button>
            <span v-else class="sv-gone"
                  title="its anchor left the page — likely addressed">§ changed</span>
            <span v-if="lastAuthor(t.id) === 'agent'" class="sv-turn-tag">agent replied</span>
            <button type="button" class="sv-twist-btn"
                    :aria-expanded="String(!collapsed[t.id])"
                    @click="toggle(t.id)">{{ collapsed[t.id] ? '▸' : '▾' }}</button>
          </div>
          <template v-if="!collapsed[t.id]">
            <blockquote v-if="t.quote">{{ t.quote }}</blockquote>
            <div v-for="c in commentsFor(t.id)" :key="c.id" class="sv-comment">
              <span class="sv-comment-meta" :class="{ 'sv-agent': c.author === 'agent' }">
                {{ c.author || 'user' }} · {{ fmt(c.created_at) }}</span>
              <div class="sv-comment-body">{{ c.body }}</div>
            </div>
            <textarea v-model="replies[t.id]" rows="1" placeholder="Reply…"
                      @keydown.meta.enter="reply(t)" @keydown.ctrl.enter="reply(t)"></textarea>
            <div class="sv-cbar-actions">
              <button type="button" @click="reply(t)">reply</button>
              <button type="button" class="sv-quiet" @click="resolve(t)"
                      title="resolve — reopenable below">resolve</button>
            </div>
          </template>
        </div>

        <details v-if="resolved.length" class="sv-cbar-resolved">
          <summary>resolved ({{ resolved.length }})</summary>
          <div v-for="t in resolved" :key="t.id" class="sv-cbar-card sv-was-resolved">
            <div class="sv-cbar-meta">
              <button v-if="attach[t.id]" type="button" class="sv-jump" @click="jump(t)">↩ {{ t.target }}</button>
              <span v-else class="sv-gone">§ changed</span>
            </div>
            <blockquote v-if="t.quote">{{ t.quote }}</blockquote>
            <div v-for="c in commentsFor(t.id)" :key="c.id" class="sv-comment">
              <span class="sv-comment-meta" :class="{ 'sv-agent': c.author === 'agent' }">
                {{ c.author || 'user' }} · {{ fmt(c.created_at) }}</span>
              <div class="sv-comment-body">{{ c.body }}</div>
            </div>
            <div class="sv-cbar-actions">
              <button type="button" class="sv-quiet" @click="reopen(t)">reopen</button>
            </div>
          </div>
        </details>
      </div>
    `,
  }).mount($bar);

  // Focus the draft box whenever a draft begins.
  watchEffect(() => {
    if (svc.draft) {
      nextTick(() => {
        $bar.querySelector('.sv-cbar-draft textarea')?.focus({ preventScroll: true });
      });
    }
  });
}

// ---- the selection chip: the one creation gesture -----------------------------
// Select any text in a block and a small "comment" chip appears; the selection
// becomes the quote, the containing element's text the context. Double-click
// works for free (it selects a word). No resting furniture in the content.

const $chip = document.createElement('button');
$chip.id = 'sv-cchip';
$chip.type = 'button';
$chip.textContent = 'comment';
$chip.hidden = true;
document.body.appendChild($chip);

let chipTimer = 0;
document.addEventListener('selectionchange', () => {
  clearTimeout(chipTimer);
  chipTimer = setTimeout(placeChip, 150);
});

function placeChip() {
  const sel = getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount) { $chip.hidden = true; return; }
  const range = sel.getRangeAt(0);
  const cont = range.commonAncestorContainer;
  const el = (cont.nodeType === 1 ? cont : cont.parentElement)?.closest('#sv-blocks [data-block]');
  if (!el) { $chip.hidden = true; return; }
  const r = range.getBoundingClientRect();
  $chip.style.top = Math.max(scrollY + r.top - 34, scrollY + 4) + 'px';
  $chip.style.left = Math.min(scrollX + r.right + 8, scrollX + innerWidth - 110) + 'px';
  $chip.hidden = false;
}

$chip.addEventListener('mousedown', (e) => e.preventDefault()); // keep the selection
$chip.addEventListener('click', () => {
  const sel = getSelection();
  if (!sel || sel.isCollapsed || !svc) return;
  const range = sel.getRangeAt(0);
  const startEl = range.startContainer.nodeType === 1
    ? range.startContainer
    : range.startContainer.parentElement;
  const spot =
    startEl?.closest(':is(h1, h2, h3, h4, h5, h6)[id], p, li, pre') ||
    startEl?.closest('[data-block]');
  const block = spot?.closest('[data-block]');
  if (!block) return;
  svc.draft = {
    target: block.dataset.block,
    anchor: anchorOf(spot),
    quote: sel.toString().trim().slice(0, 300),
    context: textOf(spot).trim().slice(0, 500),
    text: '',
  };
  sel.removeAllRanges();
  $chip.hidden = true;
  document.body.classList.add('sv-cbar-open'); // overlay screens: show the compose
});

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
const CLIENT_STAMP = '2026-08-08j no-mermaid';
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

// ---- conversation: comments from the page -----------------------------------
// The daemon ships each page's threads+comments whole (the `threads` event);
// this side places them. Placement is Sphinx's headerlink model: hover a
// heading, paragraph or block and a margin mark appears — click to comment
// there. Existing threads are a faint count-dot until their anchor is
// hovered. Resolved OR orphaned threads share the tail list: the same kind
// of thing, a conversation not attached to a live spot. Orphanhood is
// computed here (does the anchor still resolve?), never stored — so a
// returning anchor re-attaches its thread by construction.

const $tail = document.getElementById('sv-tail');
let placed = new Map(); // element -> [threads], as of the last render

function conversation() {
  return state.conversations.get(state.selected) || { threads: [], comments: [] };
}

function commentsFor(threadId) {
  return conversation().comments.filter((c) => c.thread_id === threadId);
}

// FNV-1a 64 over the whitespace-normalized text, low 48 bits as 12 hex —
// the `p:` anchor. Vector: anchorHash('the quick brown fox') = '8115ea47e2c8'.
// The daemon-side twin arrives with re-resolution (V2.sv); until then this
// is the only implementation, and the vector above is the contract.
function anchorHash(text) {
  const s = text.replace(/\s+/g, ' ').trim();
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(s)) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return (h & 0xffffffffffffn).toString(16).padStart(12, '0');
}

// textContent with our own injected UI (comment bubbles) stripped — hashing
// must see the author's text, not the decoration.
function textOf(el) {
  const clone = el.cloneNode(true);
  for (const d of clone.querySelectorAll('.sv-cdot, .sv-cmark')) d.remove();
  return clone.textContent;
}

// The comment bubble, drawn inline: an empty one (rounded, two text lines)
// trails every commentable bit, hover-revealed; a numbered one stays put
// where a thread lives, the count in place of the lines. Filled means the
// agent had the last word — the user's turn.
function bubbleSvg(count, filled) {
  const inner = count == null
    ? '<path d="M7.5 8h9M7.5 12h5.5" stroke-width="1.6"/>'
    : `<text x="12" y="10" text-anchor="middle" dominant-baseline="central"
         font-size="11" font-weight="600" stroke="none"
         fill="${filled ? 'var(--bs-body-bg)' : 'currentColor'}">${count}</text>`;
  return `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"
      fill="none" stroke="currentColor" stroke-width="1.8"
      stroke-linejoin="round" stroke-linecap="round">
    <path d="M21 14a3 3 0 0 1-3 3H8l-5 4V6a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3z"
      ${filled ? 'fill="currentColor"' : ''}/>
    ${inner}</svg>`;
}

// thread -> the element its anchor names right now, or null (orphaned).
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

// element -> the anchor string a new thread there would carry. The p: hash
// covers list items too — same grammar, same normalization.
function anchorOf(el) {
  const block = el.closest('[data-block]');
  if (!block || el === block) return '';
  if (/^H[1-6]$/.test(el.tagName) && el.id) return 'h:' + el.id;
  if (['P', 'LI', 'PRE'].includes(el.tagName)) return 'p:' + anchorHash(textOf(el));
  return '';
}

let conversationScheduled = false;
function scheduleConversation() {
  if (conversationScheduled) return;
  conversationScheduled = true;
  requestAnimationFrame(() => {
    conversationScheduled = false;
    renderConversation();
  });
}

function renderConversation() {
  keepReading(renderConversationInner);
}

function renderConversationInner() {
  for (const d of $blocks.querySelectorAll('.sv-cdot, .sv-cmark')) d.remove();
  placed = new Map();
  const tail = [];
  for (const t of conversation().threads) {
    const el = resolveAnchor(t);
    if (t.resolved_at != null) {
      tail.push({ t, orphan: !el });
    } else if (!el) {
      tail.push({ t, orphan: true });
    } else {
      if (!placed.has(el)) placed.set(el, []);
      placed.get(el).push(t);
    }
  }
  for (const [el, threads] of placed) {
    const dot = document.createElement('button');
    dot.type = 'button';
    dot.className = 'sv-cdot';
    const all = threads.flatMap((t) => commentsFor(t.id));
    const last = all[all.length - 1];
    // The agent had the last word: filled bubble, the user's turn.
    const turn = last && last.author === 'agent';
    dot.classList.toggle('sv-turn', !!turn);
    dot.innerHTML = bubbleSvg(all.length, turn);
    dot.title =
      all.length + (all.length === 1 ? ' comment' : ' comments') +
      (turn ? ' — the agent replied' : '');
    dot.addEventListener('click', (e) => {
      e.stopPropagation();
      openPopover(el, threads);
    });
    el.appendChild(dot);
  }
  // Every other commentable bit gets the empty bubble, trailing its text
  // (floating top-right for code blocks): invisible until its element is
  // hovered (tapped, on touch screens). Loose list items defer to the
  // paragraphs inside them; diff and degraded blocks keep their own rules.
  const spots = [
    ...$blocks.querySelectorAll(':is(h1, h2, h3, h4, h5, h6)[id], p, li, pre'),
    ...$blocks.querySelectorAll(':scope > [data-block]'),
  ];
  for (const el of spots) {
    if (el.tagName === 'LI' && el.querySelector(':scope > p')) continue;
    if (el.tagName === 'PRE' && el.closest('.sv-diff, .sv-degraded')) continue;
    if (el.querySelector(':scope > .sv-cdot')) continue;
    const mark = document.createElement('button');
    mark.type = 'button';
    mark.className = 'sv-cmark';
    mark.title = 'comment here';
    mark.innerHTML = bubbleSvg(null);
    mark.addEventListener('click', (e) => {
      e.stopPropagation();
      openPopover(el, placed.get(el) || []);
    });
    el.appendChild(mark);
  }
  renderTail(tail);
}

// Touch has no hover: the first tap on a commentable bit reveals its bubble
// (one at a time), the second tap — on the bubble — opens the popover.
if (matchMedia('(hover: none)').matches) {
  $blocks.addEventListener('click', (e) => {
    if (e.target.closest('.sv-cmark, .sv-cdot, a, button, input, textarea')) return;
    const el = e.target.closest(':is(h1, h2, h3, h4, h5, h6)[id], p, li, pre, #sv-blocks > [data-block]');
    if (!el) return;
    const mark = el.querySelector(':scope > .sv-cmark');
    if (!mark) return;
    for (const r of $blocks.querySelectorAll('.sv-cmark.sv-reveal')) r.classList.remove('sv-reveal');
    mark.classList.add('sv-reveal');
  });
}

// ---- the tail list: resolved OR orphaned, one list --------------------------

function renderTail(entries) {
  $tail.textContent = '';
  $tail.hidden = entries.length === 0;
  if (!entries.length) return;
  const title = document.createElement('h2');
  title.id = 'sv-tail-title';
  title.textContent = 'Conversations off the page';
  $tail.appendChild(title);
  for (const { t, orphan } of entries) {
    const item = document.createElement('article');
    item.className = 'sv-tail-item' + (t.resolved_at != null ? ' sv-resolved' : '');

    const meta = document.createElement('div');
    meta.className = 'sv-tail-meta';
    const status = t.resolved_at != null ? 'resolved' : 'orphaned';
    meta.textContent = `${status} · ${t.target}${t.anchor ? ' · ' + t.anchor : ''}`;
    item.appendChild(meta);

    if (t.quote) {
      const q = document.createElement('blockquote');
      q.className = 'sv-tail-quote';
      q.textContent = t.quote;
      item.appendChild(q);
    }
    for (const c of commentsFor(t.id)) {
      item.appendChild(commentEl(c));
    }

    const actions = document.createElement('div');
    actions.className = 'sv-tail-actions';
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'sv-pop-btn';
    if (t.resolved_at != null) {
      btn.textContent = 'reopen';
      btn.title = orphan
        ? 'reopen — stays here until its anchor returns'
        : 'reopen — reattaches to its spot on the page';
      btn.addEventListener('click', () => setResolution(t.id, true));
    } else {
      btn.textContent = 'resolve';
      btn.addEventListener('click', () => setResolution(t.id, false));
    }
    actions.appendChild(btn);
    item.appendChild(actions);
    $tail.appendChild(item);
  }
}

function commentEl(c) {
  const el = document.createElement('div');
  el.className = 'sv-comment';
  const meta = document.createElement('span');
  meta.className = 'sv-comment-meta';
  const when = new Date(c.created_at).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  });
  // Role before identity: agent or user (pre-role rows read as user).
  meta.textContent = (c.author || 'user') + ' · ' + when;
  if (c.author === 'agent') meta.classList.add('sv-agent');
  const body = document.createElement('div');
  body.className = 'sv-comment-body';
  body.textContent = c.body;
  el.append(meta, body);
  return el;
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
  // The daemon's snapshot repaints everything within a tick; no local state.
}

// ---- the popover: one conversation, or a fresh comment ----------------------

let $popover = null;

function closePopover() {
  if ($popover) { $popover.remove(); $popover = null; }
}

addEventListener('keydown', (e) => { if (e.key === 'Escape') closePopover(); });
addEventListener('click', (e) => {
  if ($popover && !$popover.contains(e.target)) closePopover();
});

// One open thread per bit, usually: the popover leads with the existing
// conversation and its reply box; "start a new thread" is the smaller
// affordance. With no threads it opens straight into composing.
function openPopover(el, threads, forceNew) {
  closePopover();
  const target = el.closest('[data-block]')?.dataset.block;
  if (!target) return;
  const anchor = anchorOf(el);

  $popover = document.createElement('div');
  $popover.id = 'sv-popover';

  const open = forceNew ? [] : threads.filter((t) => t.resolved_at == null);
  for (const t of open) {
    const wrap = document.createElement('div');
    wrap.className = 'sv-pop-thread';
    for (const c of commentsFor(t.id)) wrap.appendChild(commentEl(c));
    const resolveBtn = document.createElement('button');
    resolveBtn.type = 'button';
    resolveBtn.className = 'sv-pop-btn';
    resolveBtn.textContent = 'resolve';
    resolveBtn.title = 'resolve — undoable from the list at the page tail';
    resolveBtn.addEventListener('click', () => {
      setResolution(t.id, false);
      closePopover();
    });
    wrap.appendChild(resolveBtn);
    $popover.appendChild(wrap);
  }

  const box = document.createElement('textarea');
  box.className = 'sv-pop-box';
  box.rows = 2;
  box.placeholder = open.length ? 'Reply…' : 'Comment…';
  const send = document.createElement('button');
  send.type = 'button';
  send.className = 'sv-pop-btn sv-pop-send';
  send.textContent = open.length ? 'reply' : 'comment';
  const err = document.createElement('div');
  err.className = 'sv-pop-error';
  send.addEventListener('click', async () => {
    const body = box.value.trim();
    if (!body) return;
    const payload = open.length
      ? { thread: open[open.length - 1].id, body }
      : {
          page: state.selected,
          target,
          anchor,
          quote: anchor && el !== blockEl(target) ? textOf(el).trim().slice(0, 300) : null,
          body,
        };
    try {
      await postComment(payload);
      closePopover(); // the snapshot repaints the dots within a tick
    } catch (e) {
      err.textContent = String(e.message || e);
    }
  });
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) send.click();
  });
  $popover.append(box, send, err);

  if (open.length && !forceNew) {
    const fresh = document.createElement('button');
    fresh.type = 'button';
    fresh.className = 'sv-pop-new';
    fresh.textContent = 'start a new thread here';
    fresh.addEventListener('click', () => openPopover(el, threads, true));
    $popover.appendChild(fresh);
  }

  // Place after measuring: below the anchor when it fits, flipped above
  // when the room is above, clamped to the viewport with its own scrollbar
  // otherwise — a popover must never grow the page or drag its scroll.
  $popover.style.visibility = 'hidden';
  document.body.appendChild($popover);
  const r = el.getBoundingClientRect();
  const margin = 10;
  const roomBelow = innerHeight - r.bottom - margin;
  const roomAbove = r.top - margin;
  let ph = $popover.offsetHeight;
  let top;
  if (ph <= roomBelow) {
    top = scrollY + r.bottom + 6;
  } else if (ph <= roomAbove) {
    top = scrollY + r.top - ph - 6;
  } else {
    const below = roomBelow >= roomAbove;
    const room = Math.max(140, (below ? roomBelow : roomAbove) - 6);
    $popover.style.maxHeight = room + 'px';
    ph = $popover.offsetHeight;
    top = below ? scrollY + r.bottom + 6 : scrollY + r.top - ph - 6;
  }
  $popover.style.top = top + 'px';
  $popover.style.left = Math.min(scrollX + r.left, scrollX + innerWidth - 380) + 'px';
  $popover.style.visibility = '';
  box.focus({ preventScroll: true });
}

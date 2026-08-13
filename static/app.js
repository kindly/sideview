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
const CLIENT_STAMP = '2026-08-09B status-at-tail';
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
  view: 'page',          // 'page' | 'home'
};

let outline = { sections: [], blockSections: new Map() };
let railRefs = new Map(); // section key -> {link, twist, kids}

const $blocks = document.getElementById('sv-blocks');
const $sessions = document.getElementById('sv-sessions');
const $status = document.getElementById('sv-status');
const $brand = document.getElementById('sv-brand');
// The wordmark is the way home: the index of every page, grouped.
$brand.addEventListener('click', () => {
  if (state.view !== 'home') goHome();
});
const $outline = document.getElementById('sv-outline');
const $railToggle = document.getElementById('sv-rail-toggle');
const $outlineList = document.getElementById('sv-outline-list');

// /s/<session> pins that session; / follows the most recently active one;
// /home is the index, which follows nothing.
const pathMatch = location.pathname.match(/^\/s\/(.+)$/);
if (pathMatch) {
  state.selected = decodeURIComponent(pathMatch[1]);
  state.pinned = true;
}
// /home is the index: categories and the pages in them. It pins, because an
// index must not be yanked away by activity elsewhere.
if (location.pathname === '/home') {
  state.view = 'home';
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
  } else if (
    !state.pinned && mostActive && mostActive.id !== state.selected &&
    !(svc && svc.drafts.length)
  ) {
    // Composing holds the tab: auto-follow yanking the page out from under
    // an open draft is how thread 28 got misfiled. Same law as the reading
    // position — attention is never stolen.
    switchSession(mostActive.id);
  }
  if (state.view === 'home' && !hasCategories()) {
    state.view = 'page'; // nothing to index; the strip is the whole story
    state.pinned = false;
  }
  renderSessionStrip();
  if (state.view !== 'page') renderAllBlocks(); // indexes list pages: they follow them
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
  state.conversations.set(ev.page, {
    threads: ev.threads,
    comments: ev.comments,
    attachments: ev.attachments || [],
  });
  if (ev.page === state.selected) scheduleConversation();
});

function switchSession(id) {
  state.selected = id;
  state.section = null;
  state.spyActive = null;
  state.expand.clear();
  renderAllBlocks();
}

// Pages in display order: by category, then by the `order` a page (or the
// config) declared, then by creation. Uncategorized pages keep today's
// behaviour and sit last, under no label — the default category is "no
// category", not a category called default.
function groupedSessions() {
  const cat = (s) => ((s.props && s.props.category) || '').trim();
  const ord = (s) => {
    const o = parseFloat((s.props && s.props.order) ?? '');
    return Number.isFinite(o) ? o : Infinity;
  };
  const groups = new Map();
  for (const s of state.sessions) {
    const k = cat(s);
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(s);
  }
  for (const list of groups.values()) {
    list.sort((a, b) => ord(a) - ord(b) || state.sessions.indexOf(a) - state.sessions.indexOf(b));
  }
  // A category sorts by its earliest declared order, so `order` places
  // groups as well as pages and nobody needs a second key.
  return [...groups.entries()]
    .sort(([ka, a], [kb, b]) => {
      if (!ka !== !kb) return ka ? 1 : -1; // the untitled set leads
      const d = Math.min(...a.map(ord)) - Math.min(...b.map(ord));
      return Number.isFinite(d) && d !== 0 ? d : ka.localeCompare(kb);
    })
    .map(([name, pages]) => ({ name, pages }));
}

// The strip carries one category's pages and nothing else (author,
// 2026-08-10). Its length is the whole point: a project may hold dozens of
// pages, but you are only ever working inside one set, so the strip shows
// that set and a title naming it. Moving between categories is the home
// index's job, not the strip's — it never lists a category as a chip.
function renderSessionStrip() {
  $sessions.textContent = '';
  const groups = groupedSessions();

  // The strip always shows the siblings of what you are looking at. On the
  // home index that is the categories, and only those: the untitled set's
  // pages belong to the index below, not to the strip (author, 2026-08-10).
  if (state.view === 'home') {
    for (const g of groups) {
      if (!g.name) continue;
      const chip = document.createElement('span');
      chip.className = 'sv-chip sv-chip-cat';
      const btn = document.createElement('button');
      btn.className = 'sv-chip-label';
      btn.textContent = g.name;
      btn.title = `${g.pages.length} page${g.pages.length === 1 ? '' : 's'}`;
      btn.addEventListener('click', () => {
        const el = document.getElementById('sv-cat-' + cssId(g.name));
        if (el) el.scrollIntoView({ block: 'start', behavior: 'smooth' });
      });
      chip.appendChild(btn);
      $sessions.appendChild(chip);
    }
    return;
  }

  const here = state.sessions.find((s) => s.id === state.selected);
  const current = ((here && here.props && here.props.category) || '').trim();

  const group = groups.find((g) => g.name === current);
  if (current) {
    // A name for the set you are in. It names; it does not navigate — the
    // index is the only place categories are browsed (author, 2026-08-10).
    const title = document.createElement('span');
    title.className = 'sv-strip-title';
    title.textContent = current;
    $sessions.appendChild(title);
  }
  renderChips(group ? group.pages : groups.find((g) => !g.name)?.pages || []);
}

function hasCategories() {
  return state.sessions.some((s) => ((s.props && s.props.category) || '').trim());
}

function goHome() {
  // A project that never used categories has nothing to index: home is the
  // view it always had — every page in the strip (author, 2026-08-10).
  if (!hasCategories()) {
    state.view = 'page';
    state.pinned = false;
    history.pushState(null, '', '/');
    renderAllBlocks();
    renderSessionStrip();
    return;
  }
  state.view = 'home';
  state.pinned = true;
  history.pushState(null, '', '/home');
  renderAllBlocks();
  renderSessionStrip();
}

function renderChips(sessions) {
  for (const s of sessions) {
    const chip = document.createElement('span');
    chip.className = 'sv-chip' + (s.id === state.selected ? ' active' : '');

    const btn = document.createElement('button');
    btn.className = 'sv-chip-label';
    btn.textContent = (s.props && s.props.label) || shortLabel(s.id);
    btn.title = s.id;
    btn.addEventListener('click', () => {
      state.pinned = true;
      state.view = 'page';
      history.pushState(null, '', '/s/' + encodeURIComponent(s.id));
      switchSession(s.id);
      renderSessionStrip();
    });

    // The ✕ is tidying power, and what it tidies depends on the page's tier
    // (V3.sv): a throwaway page's file goes with it; a committed page is only
    // closed, its file left to git. A page the config declares gets no ✕ at
    // all — closing it would be a lie, since the config re-binds it.
    const props = s.props || {};
    if (props.closable === 'config') {
      chip.append(btn);
      $sessions.appendChild(chip);
      continue;
    }
    const throwaway = props.tier !== 'committed';
    const del = document.createElement('button');
    del.className = 'sv-chip-del';
    del.textContent = '×';
    const rest = throwaway ? 'delete this page' : 'close this page — the file stays';
    const armed = throwaway
      ? 'click again to delete — removes the page file'
      : 'click again to close — unbinds it; the committed file is untouched';
    del.title = rest;
    let disarm = 0;
    del.addEventListener('click', () => {
      if (!chip.classList.contains('sv-armed')) {
        chip.classList.add('sv-armed');
        del.title = armed;
        disarm = setTimeout(() => {
          chip.classList.remove('sv-armed');
          del.title = rest;
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
  const b = document.createElement('button');
  b.type = 'button';
  b.className = 'sv-block-comment';
  b.title = 'comment on this block';
  b.setAttribute('aria-label', 'comment on this block');
  b.innerHTML = BUBBLE_SVG;
  b.addEventListener('click', () => startDraft(el, ''));
  el.appendChild(b);
  // No selectable text means no other way in, so the affordance stops being
  // hover-revealed and simply sits there, quietly.
  if (el.querySelector('iframe') && !textOf(el).trim()) {
    el.classList.add('sv-needs-bubble');
  }
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

// The index: every category with the pages in it, and the untitled set
// leading. There is no per-category page — categories are browsed here and
// nowhere else (author, 2026-08-10).
function cssId(name) {
  return name.replace(/[^A-Za-z0-9_-]/g, '_');
}

function renderIndex() {
  $blocks.textContent = '';
  document.body.classList.remove('sv-rail');
  const wrap = document.createElement('section');
  wrap.className = 'sv-block sv-home';
  const h = document.createElement('h1');
  h.textContent = 'Pages';
  wrap.appendChild(h);

  const groups = groupedSessions();
  if (!groups.length) {
    const p = document.createElement('p');
    p.className = 'text-muted';
    p.textContent = 'No pages yet.';
    wrap.appendChild(p);
  }

  for (const g of groups) {
    const sec = document.createElement('div');
    sec.className = 'sv-home-group';
    if (g.name) {
      sec.id = 'sv-cat-' + cssId(g.name);
      const head = document.createElement('div');
      head.className = 'sv-home-cat';
      const name = document.createElement('span');
      name.className = 'sv-home-cat-name';
      name.textContent = g.name;
      const count = document.createElement('span');
      count.className = 'sv-home-meta';
      count.textContent = `${g.pages.length} page${g.pages.length === 1 ? '' : 's'}`;
      head.append(name, count);
      sec.appendChild(head);
    }
    for (const s2 of g.pages) {
      const a = document.createElement('a');
      a.className = 'sv-home-page';
      a.href = '/s/' + encodeURIComponent(s2.id);
      a.addEventListener('click', (e) => {
        e.preventDefault();
        state.pinned = true;
        state.view = 'page';
        history.pushState(null, '', a.getAttribute('href'));
        switchSession(s2.id);
        renderSessionStrip();
      });
      const name = document.createElement('span');
      name.className = 'sv-home-name';
      name.textContent = (s2.props && s2.props.label) || shortLabel(s2.id);
      const meta = document.createElement('span');
      meta.className = 'sv-home-meta';
      meta.textContent = (s2.props && s2.props.path) || s2.id;
      a.append(name, meta);
      sec.appendChild(a);
    }
    wrap.appendChild(sec);
  }
  $blocks.appendChild(wrap);
}

function renderAllBlocks() {
  if (state.view === 'home') {
    renderIndex();
    return;
  }
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

import('/assets/vendor/vue.esm-browser.prod.js')
  .then((m) => { vue = m; mountCommentBar(); syncConversation(); })
  .catch((e) => console.warn('sideview: comment bar disabled (vue failed to load)', e));

function conversation() {
  return (
    state.conversations.get(state.selected) || { threads: [], comments: [], attachments: [] }
  );
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
  svc.threads = conv.threads;
  svc.comments = conv.comments;
  svc.attachments = conv.attachments || [];
  const attach = {};
  for (const t of conv.threads) attach[t.id] = !!resolveAnchor(t);
  svc.attach = attach;
  if (!document.body.classList.contains('sv-selecting')) syncToggle();
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

// ---- resizable rails ----------------------------------------------------------
// Both side rails drag from their inner edge; the width is the viewer's, not
// the agent's, so it is remembered in localStorage — same law as their folds
// (V3.sv). Double-click restores the default. Desktop only: on a narrow
// screen the rail is a drawer and the bar is the whole page.

function mountGrips() {
  const wide = () => matchMedia('(min-width: 64rem)').matches;
  const grip = (id, side, cssVar, key, min, max) => {
    const g = document.createElement('div');
    g.id = id;
    g.className = 'sv-grip';
    g.setAttribute('aria-hidden', 'true');
    g.title = 'drag to resize — double-click to reset';
    document.body.appendChild(g);

    const stored = parseFloat(localStorage.getItem(key) || '');
    if (stored > 0) document.documentElement.style.setProperty(cssVar, stored + 'px');

    g.addEventListener('pointerdown', (e) => {
      if (!wide()) return;
      e.preventDefault();
      g.setPointerCapture(e.pointerId);
      document.body.classList.add('sv-resizing');
      const move = (ev) => {
        const px = side === 'left' ? ev.clientX : window.innerWidth - ev.clientX;
        document.documentElement.style.setProperty(
          cssVar,
          Math.max(min, Math.min(max, px)) + 'px'
        );
      };
      const up = () => {
        document.body.classList.remove('sv-resizing');
        g.removeEventListener('pointermove', move);
        g.removeEventListener('pointerup', up);
        g.removeEventListener('pointercancel', up);
        localStorage.setItem(
          key,
          parseFloat(getComputedStyle(document.documentElement).getPropertyValue(cssVar))
        );
      };
      g.addEventListener('pointermove', move);
      g.addEventListener('pointerup', up);
      g.addEventListener('pointercancel', up);
    });

    g.addEventListener('dblclick', () => {
      document.documentElement.style.removeProperty(cssVar);
      localStorage.removeItem(key);
    });
  };
  grip('sv-rail-grip', 'left', '--sv-rail-w', 'sv-railw', 180, 560);
  grip('sv-cbar-grip', 'right', '--sv-cbar-w', 'sv-cbarw', 220, 680);
}
mountGrips();

// ---- the mobile sheet vs the iOS keyboard --------------------------------------
// Two iOS truths (thread 35, live): body overflow:hidden does not stop touch
// scroll, and position:fixed elements keep layout-viewport size while the
// keyboard shrinks the visual viewport — so the sheet's bottom hides behind
// the keyboard and the page wanders underneath. The fixes are the classic
// pair: pin the sheet to the *visual* viewport, and lock the body by making
// it fixed (remembering the scroll to give back on close).

if (window.visualViewport) {
  const vv = window.visualViewport;
  const applyVV = () => {
    const s = document.documentElement.style;
    s.setProperty('--sv-vvh', vv.height + 'px');
    s.setProperty('--sv-vvt', vv.offsetTop + 'px');
    // The keyboard shrank the sheet: whatever is being typed into must
    // come back above the fold, or focusing at the end of a long thread
    // strands the box under the keys.
    const el = document.activeElement;
    if (el && el.tagName === 'TEXTAREA' && $bar.contains(el)) {
      el.scrollIntoView({ block: 'nearest' });
    }
  };
  vv.addEventListener('resize', applyVV);
  vv.addEventListener('scroll', applyVV);
  window.addEventListener('scroll', applyVV, { passive: true });
  applyVV();
}

// The probe (#svdebug on any page URL): live numbers from the actual device,
// because two rounds of theorizing from screenshots is enough — the v1
// mobile saga's lesson, re-learned. Reports the visual viewport, the lock,
// and where the sheet and its strip actually sit.
if (location.hash === '#svdebug') {
  const probe = document.createElement('div');
  probe.style.cssText =
    'position:fixed;left:0;top:40%;z-index:9999;background:#000;color:#0f0;' +
    'font:10px monospace;padding:4px 6px;pointer-events:none;white-space:pre;';
  document.body.appendChild(probe);
  const report = () => {
    const vv = window.visualViewport;
    const sheet = document.getElementById('sv-comments');
    const strip = document.querySelector('.sv-cbar-title');
    const scroll = document.querySelector('.sv-cbar-scroll');
    const r = (el) => (el ? `${Math.round(el.getBoundingClientRect().top)},h${Math.round(el.getBoundingClientRect().height)}` : 'none');
    probe.textContent =
      `vv ${vv ? Math.round(vv.offsetTop) + ',h' + Math.round(vv.height) : 'none'}\n` +
      `scrollY ${Math.round(window.scrollY)} lock ${document.body.style.position || 'off'}\n` +
      `sheet ${r(sheet)}\nstrip ${r(strip)}\nscroll ${r(scroll)}\n` +
      `at ${scroll ? Math.round(scroll.scrollTop) : '-'}`;
    requestAnimationFrame(report);
  };
  report();
}

let sheetLockY = -1;
function syncSheetLock() {
  const sheet =
    matchMedia('(max-width: 63.98rem)').matches &&
    document.body.classList.contains('sv-cbar') &&
    document.body.classList.contains('sv-cbar-open');
  if (sheet && sheetLockY < 0) {
    sheetLockY = window.scrollY;
    document.body.style.position = 'fixed';
    document.body.style.top = -sheetLockY + 'px';
    document.body.style.width = '100%';
  } else if (!sheet && sheetLockY >= 0) {
    document.body.style.position = '';
    document.body.style.top = '';
    document.body.style.width = '';
    window.scrollTo(0, sheetLockY);
    sheetLockY = -1;
  }
}
// Every open/close path is a body-class change — observe rather than chase.
new MutationObserver(syncSheetLock).observe(document.body, {
  attributes: true,
  attributeFilter: ['class'],
});
matchMedia('(max-width: 63.98rem)').addEventListener('change', syncSheetLock);

// ---- the bar itself: the first Vue island ------------------------------------

function mountCommentBar() {
  const { createApp, reactive, computed, watchEffect, nextTick } = vue;
  svc = reactive({ page: null, threads: [], comments: [], attachments: [], attach: {}, drafts: [] });

  // Layout classes live on body so the CSS grid can breathe around the bar.
  // Open/closed mirrors the rail: the viewer's fold is remembered per page;
  // defaults are open on wide screens, folded to the chip on small ones.
  watchEffect(() => {
    void svc.threads.length; void svc.drafts.length; void svc.page;
    applyCbarPref();
  });

  createApp({
    setup() {
      const open = computed(() => svc.threads.filter((t) => t.resolved_at == null));
      const resolved = computed(() =>
        svc.threads
          .filter((t) => t.resolved_at != null)
          .sort((a, b) => b.resolved_at - a.resolved_at) // freshest fold first
      );
      const collapsed = reactive({});
      const replies = reactive({});
      const error = reactive({ msg: '' });

      const commentsFor = (id) => svc.comments.filter((c) => c.thread_id === id);
      const lastAuthor = (id) => {
        const cs = commentsFor(id);
        return cs.length ? (cs[cs.length - 1].author || 'user') : null;
      };
      // The silence-fillers. sent: the plumbing's delivery receipt (the
      // server has it; says nothing about the agent). working: the agent's
      // own declaration for long tasks, retired by its reply.
      const sentPending = (id) => {
        const cs = commentsFor(id);
        const last = cs[cs.length - 1];
        return !!(last && last.author !== 'agent' && last.seen_at);
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
      // ---- attachments: paste/drop into any compose box (V3.sv's plan) ----
      // Uploads happen immediately; the token controls placement, membership
      // is the chip row. Every upload attaches whether or not its token
      // survives editing.
      const replyAtts = reactive({});
      const rAtts = (id) => (replyAtts[id] || (replyAtts[id] = []));
      // Images embed, everything else links: `![a.csv](…)` is a lie in a
      // body people read and edit (found live, thread 36).
      const attToken = (a) =>
        `${a.mime.startsWith('image/') ? '!' : ''}[${a.name}](att:${a.sha256.slice(0, 8)})`;
      const uploadOne = async (file) => {
        const res = await fetch(
          '/api/attachments?name=' + encodeURIComponent(file.name || 'pasted'),
          { method: 'POST', body: file }
        );
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      };
      const composeFiles = async (e, bucket, get, set) => {
        const files = [...((e.clipboardData || e.dataTransfer)?.files || [])];
        if (!files.length) return; // plain text paste stays native
        e.preventDefault();
        const el = e.target;
        for (const f of files) {
          try {
            const a = await uploadOne(f);
            bucket.push(a);
            const text = get() || '';
            const pos =
              el && typeof el.selectionStart === 'number' ? el.selectionStart : text.length;
            set(text.slice(0, pos) + attToken(a) + text.slice(pos));
          } catch (err) {
            error.msg = String(err.message || err);
          }
        }
      };
      const draftFiles = (e, d) => composeFiles(e, d.atts, () => d.text, (v) => { d.text = v; });
      const replyFiles = (e, t) =>
        composeFiles(e, rAtts(t.id), () => replies[t.id], (v) => { replies[t.id] = v; });
      // The mobile road in: no clipboard image, no drag — the native picker.
      // A real input overlays the attach button (no scripted click()): iOS
      // opens pickers reliably only for genuine input activation — a
      // detached input did nothing and a hidden DOM one was still flaky
      // (both found live, thread 34).
      const pickChange = (e, kind, obj) => {
        const files = e.target.files;
        if (!files || !files.length) return;
        const fake = { clipboardData: { files }, preventDefault() {}, target: null };
        if (kind === 'd') draftFiles(fake, obj);
        else replyFiles(fake, obj);
        e.target.value = ''; // the same file twice still fires change
      };
      const dropAtt = (bucket, a, get, set) => {
        bucket.splice(bucket.indexOf(a), 1);
        set((get() || '').replaceAll(attToken(a), ''));
      };
      const kb = (n) => (n < 1024 ? n + ' B' : Math.max(1, Math.round(n / 1024)) + ' KB');

      // Bodies are markdown, rendered server-side (comrak, safe mode) and
      // delivered as body_html; the one client job left is resolving att:
      // image URLs against the comment's own rows. Attachments whose token
      // was edited away trail the body.
      const esc = (s) =>
        s.replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
      const fUrl = (p) => '/f/' + p.split('/').map(encodeURIComponent).join('/');
      const attHtml = (a) =>
        a.mime.startsWith('image/')
          ? `<a class="sv-att-link" href="${fUrl(a.path)}" target="_blank" rel="noopener">` +
            `<img class="sv-att-img" src="${fUrl(a.path)}" alt="${esc(a.name)}" loading="lazy"></a>`
          : `<a class="sv-att-chip" href="${fUrl(a.path)}" target="_blank" rel="noopener">` +
            `${esc(a.name)} <span>${kb(a.bytes)}</span></a>`;
      const bodyHtml = (c) => {
        const atts = svc.attachments.filter((a) => a.comment_id === c.id);
        const used = new Set();
        let html = c.body_html != null ? c.body_html : esc(c.body); // older-daemon fallback
        const swap = (m, sha) => {
          const a = atts.find((x) => x.sha256.startsWith(sha) && !used.has(x.id));
          if (!a) return m;
          used.add(a.id);
          return attHtml(a);
        };
        html = html
          .replace(/<img[^>]*\bsrc="att:([0-9a-f]{8})"[^>]*\/?>/g, swap)
          .replace(/<a[^>]*\bhref="att:([0-9a-f]{8})"[^>]*>.*?<\/a>/g, swap);
        for (const a of atts) if (!used.has(a.id)) html += attHtml(a);
        return html;
      };

      const reply = (t) => {
        const body = (replies[t.id] || '').trim();
        const atts = rAtts(t.id);
        if (!body && !atts.length) return;
        send({ thread: t.id, body: body || '(attachment)', attachments: atts }, () => {
          replies[t.id] = '';
          replyAtts[t.id] = [];
        });
      };
      const sendDraft = (d) => {
        if (!d.text.trim() && !d.atts.length) return;
        send(
          { page: d.page, target: d.target, anchor: d.anchor,
            quote: d.quote || null, context: d.context || null,
            body: d.text.trim() || '(attachment)', attachments: d.atts },
          () => { svc.drafts = svc.drafts.filter((x) => x !== d); }
        );
      };

      // Compose boxes grow with their text (field-sizing isn't in Safari
      // yet); capped so a long paste never swallows the sheet.
      const grow = (e) => {
        const el = e.target;
        el.style.height = 'auto';
        el.style.height = Math.min(el.scrollHeight + 2, 320) + 'px';
      };

      const draftsHere = computed(() => svc.drafts.filter((d) => d.page === svc.page));

      return {
        svc, draftsHere, open, resolved, collapsed, replies, error,
        commentsFor, lastAuthor, sentPending, fmt, jump, reply, sendDraft,
        rAtts, draftFiles, replyFiles, bodyHtml, pickChange, grow,
        removeDraftAtt: (d, a) => dropAtt(d.atts, a, () => d.text, (v) => { d.text = v; }),
        removeReplyAtt: (t, a) =>
          dropAtt(rAtts(t.id), a, () => replies[t.id], (v) => { replies[t.id] = v; }),
        attach: computed(() => svc.attach),
        resolve: (t) => setResolution(t.id, false),
        reopen: (t) => setResolution(t.id, true),
        toggle: (id) => { collapsed[id] = !collapsed[id]; },
        cancelDraft: (d) => { svc.drafts = svc.drafts.filter((x) => x !== d); },
        fold: () => {
          document.body.classList.remove('sv-cbar-open');
          localStorage.setItem('sv-cbar:' + svc.page, 'closed');
        },
      };
    },
    template: `
      <div v-if="svc.threads.length || draftsHere.length" class="sv-cbar-inner">
        <div class="sv-cbar-title">Comments
          <button type="button" class="sv-cbar-fold" aria-label="collapse comments"
                  title="collapse — the bubble brings it back" @click="fold"></button></div>
        <div class="sv-cbar-scroll">
        <div v-if="error.msg" class="sv-cbar-error">{{ error.msg }}</div>

        <div v-for="d in draftsHere" :key="d.key" class="sv-cbar-card sv-cbar-draft">
          <blockquote v-if="d.quote">{{ d.quote }}</blockquote>
          <textarea v-model="d.text" rows="4" placeholder="Comment… (paste or drop files)"
                    :data-draft="d.key" @input="grow"
                    @paste="draftFiles($event, d)"
                    @drop.prevent="draftFiles($event, d)" @dragover.prevent
                    @keydown.meta.enter="sendDraft(d)" @keydown.ctrl.enter="sendDraft(d)"
                    @keydown.esc="cancelDraft(d)"></textarea>
          <div v-if="d.atts.length" class="sv-att-row">
            <span v-for="a in d.atts" :key="a.sha256 + a.name" class="sv-att-pending">{{ a.name }}
              <button type="button" aria-label="remove attachment"
                      @click="removeDraftAtt(d, a)">×</button></span>
          </div>
          <div class="sv-cbar-actions">
            <button type="button" @click="sendDraft(d)">comment</button>
            <label class="sv-attach-btn" title="attach files — paste and drop work too">attach
              <input type="file" multiple @change="pickChange($event, 'd', d)"></label>
            <button type="button" class="sv-quiet" @click="cancelDraft(d)">cancel</button>
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
            <button type="button" class="sv-twist-btn"
                    :aria-expanded="String(!collapsed[t.id])"
                    @click="toggle(t.id)">{{ collapsed[t.id] ? '▸' : '▾' }}</button>
          </div>
          <template v-if="!collapsed[t.id]">
            <blockquote v-if="t.quote">{{ t.quote }}</blockquote>
            <div v-for="c in commentsFor(t.id)" :key="c.id" class="sv-comment">
              <span class="sv-comment-meta" :class="{ 'sv-agent': c.author === 'agent' }">
                {{ c.author || 'user' }} · {{ fmt(c.created_at) }}</span>
              <div class="sv-comment-body" v-html="bodyHtml(c)"></div>
            </div>
            <div v-if="t.working_at" class="sv-status sv-working"
                 title="the agent marked this as in progress">working…</div>
            <div v-else-if="sentPending(t.id)" class="sv-status"
                 title="delivered — the server has it; an agent hasn't necessarily read it yet">sent</div>
            <textarea v-model="replies[t.id]" rows="3" placeholder="Reply… (paste or drop files)"
                      @input="grow"
                      @paste="replyFiles($event, t)"
                      @drop.prevent="replyFiles($event, t)" @dragover.prevent
                      @keydown.meta.enter="reply(t)" @keydown.ctrl.enter="reply(t)"></textarea>
            <div v-if="rAtts(t.id).length" class="sv-att-row">
              <span v-for="a in rAtts(t.id)" :key="a.sha256 + a.name" class="sv-att-pending">{{ a.name }}
                <button type="button" aria-label="remove attachment"
                        @click="removeReplyAtt(t, a)">×</button></span>
            </div>
            <div class="sv-cbar-actions">
              <button type="button" @click="reply(t)">reply</button>
              <label class="sv-attach-btn" title="attach files — paste and drop work too">attach
                <input type="file" multiple @change="pickChange($event, 'r', t)"></label>
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
              <div class="sv-comment-body" v-html="bodyHtml(c)"></div>
            </div>
            <div class="sv-cbar-actions">
              <button type="button" class="sv-quiet" @click="reopen(t)">reopen</button>
            </div>
          </div>
        </details>
        </div>
      </div>
    `,
  }).mount($bar);

}

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

const $chip = document.createElement('button');
$chip.id = 'sv-cchip';
$chip.type = 'button';
$chip.setAttribute('aria-label', 'comment on the selection');
$chip.title = 'comment on the selection';
$chip.innerHTML = BUBBLE_SVG;
$chip.hidden = true;
document.body.appendChild($chip);

let chipTimer = 0;
document.addEventListener('selectionchange', () => {
  clearTimeout(chipTimer);
  chipTimer = setTimeout(placeChip, 150);
});

// The selection, remembered: tapping any affordance collapses the live
// selection first on touch, so drafts read from here. A grace timer keeps
// it briefly after collapse — long enough for the in-flight tap.
let lastSel = null;
let selClearTimer = 0;

function placeChip() {
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
    clearTimeout(selClearTimer);
    selClearTimer = setTimeout(() => {
      lastSel = null;
      document.body.classList.remove('sv-selecting');
      syncToggle();
    }, 700);
    return;
  }
  clearTimeout(selClearTimer);
  lastSel = { spot, text: sel.toString().trim() };
  if (TOUCH) {
    document.body.classList.add('sv-selecting');
    $cbarToggle.classList.add('sv-sel');
    $cbarToggle.title = 'comment on the selection';
    return;
  }
  const range = sel.getRangeAt(0);
  const rects = range.getClientRects();
  const r = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
  $chip.style.top = scrollY + r.bottom + 8 + 'px';
  $chip.style.left = Math.min(scrollX + r.right + 4, scrollX + innerWidth - 44) + 'px';
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
    $bar.querySelector(`textarea[data-draft="${key}"]`)?.focus({ preventScroll: true });
  });
}

function spotFrom(node) {
  const el = node instanceof Element ? node : node?.parentElement;
  return (
    el?.closest(':is(h1, h2, h3, h4, h5, h6)[id], p, li, pre') ||
    el?.closest('#sv-blocks [data-block]')
  );
}

$chip.addEventListener('mousedown', (e) => e.preventDefault()); // keep the selection
$chip.addEventListener('click', () => {
  if (!lastSel) return;
  const held = lastSel;
  lastSel = null;
  startDraft(held.spot, held.text);
});

// Double-click is the primary gesture (author, 2026-08-08 — the selection
// chip's position fought the browser's own selection UI): straight to a
// draft on the clicked bit, its whole text as the quote — unless a larger
// selection exists, which wins for precision.
$blocks.addEventListener('dblclick', (e) => {
  if (e.target.closest('a, button, input, textarea, iframe, #sv-comments')) return;
  const spot = spotFrom(e.target);
  if (!spot) return;
  const sel = getSelection();
  const selText = sel && !sel.isCollapsed ? sel.toString().trim() : '';
  startDraft(spot, selText.length > 20 ? selText : textOf(spot).trim());
});

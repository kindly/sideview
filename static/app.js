// The whole client. The daemon sends rendered HTML plus each block's declared
// headings; the job here is: hold an EventSource, place/replace/remove elements
// by block id, build the contents rail from declarations, and track where the
// reader is. The rail has two coherent modes, chosen per session by the agent
// (`session set --outline`): scrollspy (default — the page is always the whole
// document, the rail follows the scroll) and tabs (sections are separate
// panes — the mode for prototypes and app-like pages).
'use strict';

const state = {
  sessions: [],          // [{id, last_active_at, props}] most recent first
  blocks: new Map(),     // session id -> Map(block id -> {ord, html, headings})
  selected: null,        // session id
  pinned: false,         // the user deliberately clicked a session
  section: null,         // tabs mode: the selected section key
  spyActive: null,       // scrollspy mode: the section currently in view
  expand: new Map(),     // section key -> bool, manual twist overrides (per session)
  connectedAt: 0,        // when the stream last opened; gates the arrival ink
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

// Mermaid renders client-side (it only exists as browser JS), themed once at
// load; startOnLoad off because blocks arrive over SSE, not with the page.
if (window.mermaid) {
  window.mermaid.initialize({
    startOnLoad: false,
    theme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'default',
  });
}

// comrak renders a ```mermaid fence as <pre><code class="language-mermaid">;
// swap those for holders mermaid can render into, and also accept the direct
// spellings (<pre class="mermaid">) an agent might write in a markup block.
// A bad diagram shows mermaid's own error in place — visible beats silent.
function renderMermaid(el) {
  if (!window.mermaid) return;
  const nodes = [];
  for (const code of el.querySelectorAll('pre > code.language-mermaid')) {
    const holder = document.createElement('div');
    holder.className = 'sv-mermaid';
    holder.textContent = code.textContent;
    code.closest('pre').replaceWith(holder);
    nodes.push(holder);
  }
  for (const direct of el.querySelectorAll('pre.mermaid, div.mermaid')) {
    direct.classList.add('sv-mermaid');
    nodes.push(direct);
  }
  if (nodes.length) window.mermaid.run({ nodes }).catch(() => {});
}

const es = new EventSource('/events');
es.addEventListener('open', () => {
  state.connectedAt = Date.now();
  document.body.classList.remove('sv-disconnected');
  $status.hidden = true;
  $brand.title = 'connected';
});
es.addEventListener('error', () => {
  // EventSource reconnects on its own; the dot goes hollow while it does.
  document.body.classList.add('sv-disconnected');
  $status.hidden = false;
  $brand.title = 'reconnecting';
});

es.addEventListener('sessions', (e) => {
  state.sessions = JSON.parse(e.data).sessions;
  if (!state.pinned && state.sessions.length) {
    const top = state.sessions[0].id;
    if (top !== state.selected) {
      switchSession(top);
    }
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
  }
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
    const btn = document.createElement('button');
    btn.textContent = (s.props && s.props.label) || shortLabel(s.id);
    btn.title = s.id;
    btn.classList.toggle('active', s.id === state.selected);
    btn.addEventListener('click', () => {
      state.pinned = true;
      history.pushState(null, '', '/s/' + encodeURIComponent(s.id));
      switchSession(s.id);
      renderSessionStrip();
    });
    $sessions.appendChild(btn);
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
        sections.push({ key: id + '/' + sections.length, block: id, title: h.text, children: [] });
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

function goToSection(s) {
  if (railMode() === 'tabs') {
    state.section = s.key;
    applyVisibility();
    styleRail();
    scrollTo({ top: 0 });
  } else {
    blockEl(s.block)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
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
  return $blocks.querySelector(`[data-block="${CSS.escape(id)}"]`);
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
    const el = blockEl(s.block);
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
  const existing = blockEl(ev.block);
  if (ev.action === 'remove') {
    if (existing) existing.remove();
    return;
  }
  const el = elementFor(ev.block, ev.html, ev.ord);
  if (!el) return;
  if (existing) {           // update: patch in place, nothing scrolls or reflows around it
    existing.replaceWith(el);
    activateScripts(el);
    renderMermaid(el);
    return;
  }
  // Place by ord so the client never needs to know about neighbours.
  const next = [...$blocks.children].find((c) => (c.dataset.ord || '') > ev.ord);
  const atBottom = window.innerHeight + window.scrollY >= document.body.offsetHeight - 48;
  if (next) $blocks.insertBefore(el, next);
  else $blocks.appendChild(el);
  // The arrival ink marks genuinely-live blocks, not the replay burst that
  // follows every (re)connect.
  if (Date.now() - state.connectedAt > 1500) el.classList.add('sv-arrive');
  activateScripts(el);
  renderMermaid(el);
  // Provisional: follow new content only when already at the bottom, never
  // yank away while reading above. V0.md leaves this to be decided by feel.
  if (!next && atBottom && railMode() === 'scrollspy') {
    el.scrollIntoView({ block: 'end', behavior: 'smooth' });
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
        $blocks.appendChild(el);
        activateScripts(el);
        renderMermaid(el);
      }
    }
  }
  refreshOutline();
}

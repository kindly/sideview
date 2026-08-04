// The whole client. The daemon sends rendered HTML plus each block's declared
// headings, so the job here is only: hold an EventSource, place/replace/remove
// elements by block id, build the outline sidebar from declarations, switch
// sessions and sections, and show a banner when disconnected.
'use strict';

const state = {
  sessions: [],          // [{id, label, last_active_at}] most recent first
  blocks: new Map(),     // session id -> Map(block id -> {ord, html, headings})
  selected: null,        // session id
  pinned: false,         // the user deliberately clicked a session
  section: 'all',        // outline tab: 'all' or a section key
};

const $blocks = document.getElementById('sv-blocks');
const $sessions = document.getElementById('sv-sessions');
const $banner = document.getElementById('sv-banner');
const $outline = document.getElementById('sv-outline');
const $outlineList = document.getElementById('sv-outline-list');
const $outlineToggle = document.getElementById('sv-outline-toggle');

// Outline preference, in precedence order: the viewer's explicit toggle
// (localStorage, per page), then the agent's declared session property
// (`sideview session set --outline off`), then auto (on).
function outlinePref() {
  const stored = localStorage.getItem('sv-outline:' + state.selected);
  if (stored === 'on' || stored === 'off') return stored;
  const s = state.sessions.find((x) => x.id === state.selected);
  return s && s.props && s.props.outline === 'off' ? 'off' : 'on';
}

$outlineToggle.addEventListener('click', () => {
  localStorage.setItem(
    'sv-outline:' + state.selected,
    outlinePref() === 'off' ? 'on' : 'off'
  );
  refreshOutline();
});

// /s/<session> pins that session; / follows the most recently active one.
const pathMatch = location.pathname.match(/^\/s\/(.+)$/);
if (pathMatch) {
  state.selected = decodeURIComponent(pathMatch[1]);
  state.pinned = true;
}

const es = new EventSource('/events');
es.addEventListener('open', () => { $banner.hidden = true; });
es.addEventListener('error', () => { $banner.hidden = false; }); // EventSource reconnects on its own

es.addEventListener('sessions', (e) => {
  state.sessions = JSON.parse(e.data).sessions;
  if (!state.pinned && state.sessions.length) {
    const top = state.sessions[0].id;
    if (top !== state.selected) {
      state.selected = top;
      state.section = 'all';
      renderAllBlocks();
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

function renderSessionStrip() {
  $sessions.textContent = '';
  for (const s of state.sessions) {
    const btn = document.createElement('button');
    btn.textContent = (s.props && s.props.label) || shortLabel(s.id);
    btn.title = s.id;
    btn.classList.toggle('active', s.id === state.selected);
    btn.addEventListener('click', () => {
      state.selected = s.id;
      state.pinned = true;
      state.section = 'all';
      history.pushState(null, '', '/s/' + encodeURIComponent(s.id));
      renderSessionStrip();
      renderAllBlocks();
    });
    $sessions.appendChild(btn);
  }
}

function shortLabel(id) {
  return id.length > 12 ? id.slice(0, 8) + '…' : id;
}

// ---- the outline ------------------------------------------------------------
// Sections are blocks that declare an h1/h2; deeper headings nest under the
// section in force, and a headingless block belongs to the section it follows.
// Blocks before any section are front matter and stay visible on every tab.

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
  const outline = computeOutline();
  const off = outlinePref() === 'off';
  if (off || !outline.sections.some((s) => s.key === state.section)) {
    state.section = 'all'; // no menu, no tabs
  }

  $outlineList.textContent = '';
  const item = (label, indentClass, onclick) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'list-group-item list-group-item-action py-1 text-truncate ' + indentClass;
    btn.textContent = label;
    btn.title = label;
    btn.addEventListener('click', onclick);
    $outlineList.appendChild(btn);
    return btn;
  };

  item('— whole plan —', '', () => selectSection('all'))
    .classList.toggle('active', state.section === 'all');
  for (const s of outline.sections) {
    item(s.title, '', () => selectSection(s.key))
      .classList.toggle('active', state.section === s.key);
    for (const c of s.children) {
      item(c.text, 'sv-outline-child', () => {
        selectSection(s.key);
        const target = (c.id && document.getElementById(c.id))
          || $blocks.querySelector(`[data-block="${CSS.escape(c.block)}"]`);
        target?.scrollIntoView({ block: 'start' });
      });
    }
  }

  const hasSections = outline.sections.length > 1;
  $outline.classList.toggle('sv-has-sections', hasSections && !off);
  $outlineToggle.hidden = !hasSections;
  $outlineToggle.classList.toggle('active', !off);
  $outlineToggle.setAttribute('aria-pressed', String(!off));
  applyVisibility(outline);
}

function selectSection(key) {
  state.section = key;
  refreshOutline(); // rebuilds the list, restyles active, re-applies visibility
  scrollTo({ top: 0 });
}

function applyVisibility(outline) {
  const selIdx = outline.sections.findIndex((s) => s.key === state.section);
  for (const el of $blocks.children) {
    const memberOf = outline.blockSections.get(el.dataset.block);
    const visible =
      state.section === 'all' || !memberOf || memberOf.size === 0 || memberOf.has(selIdx);
    el.style.display = visible ? '' : 'none';
  }
}

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
  const existing = $blocks.querySelector(`[data-block="${CSS.escape(ev.block)}"]`);
  if (ev.action === 'remove') {
    if (existing) existing.remove();
    return;
  }
  const el = elementFor(ev.block, ev.html, ev.ord);
  if (!el) return;
  if (existing) {           // update: patch in place, nothing scrolls or reflows around it
    existing.replaceWith(el);
    activateScripts(el);
    return;
  }
  // Place by ord so the client never needs to know about neighbours.
  const next = [...$blocks.children].find((c) => (c.dataset.ord || '') > ev.ord);
  const atBottom = window.innerHeight + window.scrollY >= document.body.offsetHeight - 48;
  if (next) $blocks.insertBefore(el, next);
  else $blocks.appendChild(el);
  activateScripts(el);
  // Provisional: follow new content only when already at the bottom, never
  // yank away while reading above. V0.md leaves this to be decided by feel.
  if (!next && atBottom && state.section === 'all') {
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
      }
    }
  }
  refreshOutline();
}

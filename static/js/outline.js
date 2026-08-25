// The contents rail: derived from block headings (or the explicit spec),
// scrollspy or tabs.
import { state, pageProps, railMode, railOpen } from './state.js';
import { $blocks, $outline, $railToggle, $outlineList, blockEl } from './dom.js';

let outline = { sections: [], blockSections: new Map() };
let railRefs = new Map(); // section key -> {link, twist, kids}

$railToggle.addEventListener('click', () => {
  localStorage.setItem('sv-outline:' + state.selected, railOpen() ? 'off' : 'on');
  refreshOutline();
});

// ---- the contents rail --------------------------------------------------------
// Sections are blocks that declare an h1/h2; deeper headings nest under the
// section in force, and a headingless block belongs to the section it follows.
// Blocks before any section are front matter, visible on every tab.

function computeOutline() {
  // An explicit outline (sideview outline → outline_spec prop) is used
  // verbatim: the agent's ordered list, inference off. Prose derivation
  // below stays the default.
  const spec = pageProps().outline_spec;
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

export { refreshOutline };


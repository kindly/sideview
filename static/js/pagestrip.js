// The header strip and the home index: pages, grouped by category.
import { state, ROUTE } from './state.js';
import { $pages, $blocks } from './dom.js';

// Pages in display order: by category, then by the `order` a page (or the
// config) declared, then by creation. Uncategorized pages keep today's
// behaviour and sit last, under no label — the default category is "no
// category", not a category called default.
function groupedPages() {
  const cat = (s) => ((s.props && s.props.category) || '').trim();
  const ord = (s) => {
    const o = parseFloat((s.props && s.props.order) ?? '');
    return Number.isFinite(o) ? o : Infinity;
  };
  const groups = new Map();
  for (const s of state.pages) {
    const k = cat(s);
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(s);
  }
  for (const list of groups.values()) {
    list.sort((a, b) => ord(a) - ord(b) || state.pages.indexOf(a) - state.pages.indexOf(b));
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
function renderPageStrip() {
  $pages.textContent = '';
  const groups = groupedPages();

  // The strip always shows the siblings of what you are looking at. On the
  // home index that is the categories, and only those: the untitled set's
  // pages belong to the index below, not to the strip (author, 2026-08-10).
  if (ROUTE.view === 'home') {
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
      $pages.appendChild(chip);
    }
    return;
  }

  const here = state.pages.find((s) => s.id === state.selected);
  const current = ((here && here.props && here.props.category) || '').trim();

  const group = groups.find((g) => g.name === current);
  if (current) {
    // A name for the set you are in. It names; it does not navigate — the
    // index is the only place categories are browsed (author, 2026-08-10).
    const title = document.createElement('span');
    title.className = 'sv-strip-title';
    title.textContent = current;
    $pages.appendChild(title);
  }
  renderChips(group ? group.pages : groups.find((g) => !g.name)?.pages || []);
}

function renderChips(pages) {
  for (const s of pages) {
    const chip = document.createElement('span');
    chip.className = 'sv-chip' + (s.id === state.selected ? ' active' : '');

    // A chip is a link: the server routes, the browser remembers (back,
    // middle-click, bookmark — all free once nothing intercepts them).
    const btn = document.createElement('a');
    btn.className = 'sv-chip-label';
    btn.textContent = (s.props && s.props.label) || shortLabel(s.id);
    btn.title = s.id;
    btn.href = '/p/' + encodeURIComponent(s.id);

    // The ✕ is tidying power, and what it tidies depends on the page's tier
    // (V3.sv): a throwaway page's file goes with it; a committed page is only
    // closed, its file left to git. A page the config declares gets no ✕ at
    // all — closing it would be a lie, since the config re-binds it.
    const props = s.props || {};
    if (props.closable === 'config') {
      chip.append(btn);
      $pages.appendChild(chip);
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
      fetch('/api/pages/' + encodeURIComponent(s.id), { method: 'DELETE' }).catch(() => {});
    });

    chip.append(btn, del);
    $pages.appendChild(chip);
  }
}

function shortLabel(id) {
  return id.length > 12 ? id.slice(0, 8) + '…' : id;
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

  const groups = groupedPages();
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
      a.href = '/p/' + encodeURIComponent(s2.id); // a real link, nothing intercepted
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

export { groupedPages, renderPageStrip, renderIndex };


// Shared client state and the page-level facts derived from it.
// The split (V5.sv step 4): one module per concern, served as-is — no build.

// Bumped by hand whenever client behaviour changes: the daemon's version
// skew warns loudly, but a stale tab's JS is invisible — this stamp (console
// + the brand tooltip) is how you tell which client a tab is running.
const CLIENT_STAMP = '2026-08-25A ES modules';
console.log('sideview client', CLIENT_STAMP);

const state = {
  pages: [],             // [{id, last_active_at, props}] most recent first
  blocks: new Map(),     // page id -> Map(block id -> {ord, html, headings})
  selected: null,        // page id — set once from the URL, never changed
  section: null,         // tabs mode: the selected section key
  spyActive: null,       // scrollspy mode: the section currently in view
  expand: new Map(),     // section key -> bool, manual twist overrides (per page)
  connectedAt: 0,        // when the stream last opened; gates the arrival ink
  conversations: new Map(), // page id -> {threads: [], comments: []} from SSE
};

// Routing is the server's (author, 2026-08-18: a week of navigation-state
// bugs — a cross-page misfiled comment, the wrong-category strip, a dead
// back button — all impossible when the URL *is* the state). The client
// parses its location once and never navigates itself: every link is a
// real link, back is the browser's, and nothing is held across pages.
// '/' never reaches here with pages present — the server 302s it to the
// most recently active page.
const ROUTE = (() => {
  const m = location.pathname.match(/^\/p\/(.+)$/);
  if (m) return { view: 'page', page: decodeURIComponent(m[1]) };
  if (location.pathname === '/home') return { view: 'home', page: null };
  return { view: 'page', page: null }; // '/' in an empty project
})();
state.selected = ROUTE.page;

function pageProps() {
  const s = state.pages.find((x) => x.id === state.selected);
  return (s && s.props) || {};
}

// Editing from the page is an .sv affordance: block splices don't exist on
// imported md/html pages (found live on a bound .md, 2026-08-23), so the
// edit verbs are withheld there. Absent format = older daemon: assume sv.
function pageEditable() {
  const s = state.pages.find((x) => x.id === state.selected);
  return !s || !s.format || s.format === 'sv';
}


// The agent's declared mode: tabs, or scrollspy (the default). `off` means
// scrollspy with the rail starting collapsed.
function railMode() {
  return pageProps().outline === 'tabs' ? 'tabs' : 'scrollspy';
}

// Whether the rail is open: the viewer's own fold/unfold (remembered per
// page) wins over the agent's `--outline off`.
function railOpen() {
  const stored = localStorage.getItem('sv-outline:' + state.selected);
  if (stored === 'on' || stored === 'off') return stored === 'on';
  return pageProps().outline !== 'off';
}

function conversation() {
  return (
    state.conversations.get(state.selected) || { threads: [], comments: [], attachments: [] }
  );
}

// Nothing of ours lives inside block content any more (the bar is outside,
// the chip is body-level) — and it must stay that way, or content hashes
// would see decoration.
function textOf(el) {
  return el.textContent;
}

export { CLIENT_STAMP, state, ROUTE, pageProps, pageEditable, railMode, railOpen, conversation, textOf };


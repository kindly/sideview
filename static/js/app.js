// The whole client. The daemon sends rendered HTML plus each block's declared
// headings; the job here is: hold an EventSource, place/replace/remove elements
// by block id, build the contents rail from declarations, and track where the
// reader is. The rail has two coherent modes, chosen per page by the agent
// (`page set --outline`): scrollspy (default — the page is always the whole
// document, the rail follows the scroll) and tabs (sections are separate
// panes — the mode for prototypes and app-like pages).
// The entry: modules for every concern, served as-is (V5.sv step 4 — no
// build step; the daemon stamps the /assets/js/<v>/ directory so relative
// imports can never pair a fresh entry with stale submodules).
import { ROUTE } from './state.js';
import { $brand } from './dom.js';
import './theme.js';
import './scroll.js';
import './anchors.js';
import './editor.js';
import './ask.js';
import './pagestrip.js';
import './outline.js';
import './blocks.js';
import './conversation.js';
import './commentbar.js';
import './chip.js';
import './mobile.js';
import './sse.js';

// The wordmark is the way home: the index of every page, grouped.
// Real navigation — the server owns routing (author, 2026-08-18).
$brand.addEventListener('click', () => {
  if (ROUTE.view !== 'home') location.href = '/home';
});


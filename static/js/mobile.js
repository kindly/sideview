// The mobile sheet vs the iOS keyboard, the resizable rails, the #svdebug
// probe. All hard-won: see the two saga notes in HANDOFF before touching.
import { $bar } from './dom.js';

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


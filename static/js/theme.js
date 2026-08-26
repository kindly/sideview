
// The settings menu (V6.sv rounds 2–3): the header's theme button grew into
// a small popover holding the viewer's name and the dark mode switch. The
// switch is the whole theme model (round 3): a first visit follows the OS
// silently, the first flip stores light/dark and that's that — no explicit
// auto state, no reset. svTheme() lives in index.html's pre-paint script.
// One known gap: iframe-isolated html blocks theme off the OS, not the
// override (a sandboxed srcdoc can't see our localStorage).
const $settings = document.getElementById('sv-settings');
const $dark = document.getElementById('sv-set-dark');
const $name = document.getElementById('sv-set-name');

function renderSettings() {
  $dark.checked = document.documentElement.getAttribute('data-bs-theme') === 'dark';
  $name.value = localStorage.getItem('sv-name') || '';
}

function applyTheme() {
  svTheme();
  renderSettings();
  // Class-based highlighting and CSS variables re-theme by stylesheet alone;
  // only the sandboxed iframes need telling, over the envelope.
  const mode = document.documentElement.getAttribute('data-bs-theme');
  for (const f of document.querySelectorAll('iframe.sv-html')) {
    f.contentWindow?.postMessage({ sv: 1, type: 'theme', mode }, '*');
  }
}

$dark.addEventListener('change', () => {
  localStorage.setItem('sv-theme', $dark.checked ? 'dark' : 'light');
  applyTheme();
});

// The name (V6.sv round 1): typed once, kept in this browser, sent with
// every comment by postComment. Saved as typed — the daemon normalizes
// (whitespace, the 60-char cap), and clearing it goes back to unnamed.
const saveName = () => {
  const v = $name.value.trim();
  if (v) localStorage.setItem('sv-name', v);
  else localStorage.removeItem('sv-name');
};
$name.addEventListener('change', saveName);
$name.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    saveName();
    $settings.open = false;
  }
});

// A popover closes when the eye leaves: outside click or Esc. Re-rendered
// on every open so an OS theme change while it sat closed can't go stale.
document.addEventListener('click', (e) => {
  if ($settings.open && !$settings.contains(e.target)) $settings.open = false;
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && $settings.open) $settings.open = false;
});
$settings.addEventListener('toggle', () => {
  if ($settings.open) renderSettings();
});
renderSettings();

// A first visit starts with the menu open (round 3's rider): the name field
// introduces itself to a guest who just followed a link. Any interaction —
// outside click, Esc, Enter — closes it, and it never auto-opens again.
if (!localStorage.getItem('sv-welcomed')) {
  localStorage.setItem('sv-welcomed', '1');
  $settings.open = true;
}

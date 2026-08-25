
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


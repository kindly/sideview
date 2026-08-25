// Link-and-evaluate the client module graph under a minimal DOM stub.
// Born from the v5 split's first live failure (drill r5): `node --check`
// parses .js as CommonJS and missed an import redeclaration that killed the
// whole graph in the browser. This catches what a per-file parse cannot:
// import/export mismatches, redeclarations, and top-level evaluation crashes.
// Maintainer-time only — no build step enters the product.
//
//   node tools/jsgraph-check.mjs
import { cpSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'sv-jsgraph-'));
cpSync('static/js', join(dir, 'js'), { recursive: true });
writeFileSync(join(dir, 'package.json'), '{"type":"module"}');
// The vendor imports resolve to a stub: this checks OUR graph, not Vue's.
writeFileSync(join(dir, 'vendor-stub.mjs'), 'export const Idiomorph = { morph() {} };\n');
for (const f of readdirSync(join(dir, 'js'))) {
  const p = join(dir, 'js', f);
  writeFileSync(p, readFileSync(p, 'utf8').replace(/'\/assets\/vendor\/[^']+'/g, "'../vendor-stub.mjs'"));
}

const el = () => ({
  addEventListener() {}, setAttribute() {}, appendChild() {}, append() {},
  classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
  style: { setProperty() {}, removeProperty() {} },
  querySelector() { return null; }, querySelectorAll() { return []; },
  dataset: {}, textContent: '', title: '', hidden: false, innerHTML: '',
});
globalThis.document = {
  getElementById: () => el(), createElement: () => el(),
  addEventListener() {}, querySelectorAll() { return []; }, querySelector() { return null; },
  documentElement: Object.assign(el(), { getAttribute() { return null; } }),
  body: el(), activeElement: null, fonts: { ready: Promise.resolve() },
};
globalThis.location = { pathname: '/p/test', hash: '', href: '' };
globalThis.localStorage = { getItem() { return null; }, setItem() {}, removeItem() {} };
globalThis.sessionStorage = globalThis.localStorage;
globalThis.matchMedia = () => ({ matches: false, addEventListener() {} });
globalThis.addEventListener = () => {};
globalThis.history = { scrollRestoration: 'auto' };
globalThis.EventSource = class { addEventListener() {} };
globalThis.getSelection = () => null;
globalThis.MutationObserver = class { observe() {} };
globalThis.visualViewport = undefined;
globalThis.scrollY = 0; globalThis.scrollX = 0;
globalThis.innerHeight = 800; globalThis.innerWidth = 1200;
globalThis.svTheme = () => {};
globalThis.CSS = { escape: (s) => s };
globalThis.requestAnimationFrame = () => {};
globalThis.window = globalThis;

try {
  await import(join(dir, 'js', 'app.js'));
  // Let the vendor dynamic import settle (its failure path logs and degrades
  // — that message below is the stub, not a defect).
  await new Promise((r) => setTimeout(r, 50));
  console.log('GRAPH OK: every module linked and evaluated');
} catch (e) {
  console.error(`GRAPH FAIL: ${e.constructor.name}: ${e.message}`);
  process.exit(1);
}

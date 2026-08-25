// The shell's fixed elements, resolved once. Everything dynamic hangs off these.

const $blocks = document.getElementById('sv-blocks');
const $pages = document.getElementById('sv-pages');
const $status = document.getElementById('sv-status');
const $brand = document.getElementById('sv-brand');
const $outline = document.getElementById('sv-outline');
const $railToggle = document.getElementById('sv-rail-toggle');
const $outlineList = document.getElementById('sv-outline-list');
const $bar = document.getElementById('sv-comments');

function blockEl(id) {
  return id ? $blocks.querySelector(`[data-block="${CSS.escape(id)}"]`) : null;
}

export { $blocks, $pages, $status, $brand, $outline, $railToggle, $outlineList, $bar, blockEl };


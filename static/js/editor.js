// Editing from the page: prose splices with the from-hash guard; everything
// else becomes a kind='edit' request the agent merges.
import { state } from './state.js';
import { postComment } from './conversation.js';
import { applyBlock } from './blocks.js';

// ---- editing blocks from the page (V4.sv, threads 62–63) -----------------------
// Two tiers, the one-author rule applied: prose splices straight into the
// file (from-hash guard — a 409 means the agent moved the text meanwhile),
// while every other type sends a kind='edit' comment the agent merges.
// Entry is the chip's second verb; while an editor is open, SSE patches for
// that block are held, and the guard turns the remaining race into a
// warning instead of lost work.

const pendingBlockEv = new Map(); // block id -> the SSE patch held while its editor is open

async function startEdit(spot, at) {
  const block = spot?.closest('[data-block]');
  if (!block) return;
  let src;
  try {
    const res = await fetch(
      '/api/source?page=' +
        encodeURIComponent(state.selected) +
        '&block=' +
        encodeURIComponent(block.dataset.block)
    );
    if (!res.ok) throw new Error(await res.text());
    src = await res.json();
  } catch (err) {
    console.warn('sideview: could not open the editor', err);
    return;
  }
  openEditor(block, src, at);
}

function openEditor(blockEl, src, at) {
  if (blockEl.querySelector('.sv-editor')) return;
  const prose = src.type === 'sv-prose';
  const ed = document.createElement('div');
  ed.className = 'sv-editor';
  const note = document.createElement('div');
  note.className = 'sv-editor-note';
  note.textContent = prose
    ? 'editing ' + blockEl.dataset.block + ' — saves straight into the page file'
    : 'this block is ' +
      (src.type || 'code').replace('sv-', '') +
      ' — describe the change in markdown; it reaches the agent as an edit request to merge';
  const ta = document.createElement('textarea');
  ta.className = 'sv-editor-text';
  ta.value = prose ? src.body : '';
  ta.rows = Math.min(24, Math.max(6, src.body.split('\n').length + 2));
  if (!prose) ta.placeholder = 'what should change?';
  const warn = document.createElement('div');
  warn.className = 'sv-editor-warn';
  warn.hidden = true;
  const actions = document.createElement('div');
  actions.className = 'sv-editor-actions';
  const save = document.createElement('button');
  save.type = 'button';
  save.className = 'btn btn-sm btn-primary';
  save.textContent = prose ? 'save' : 'send to the agent';
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.className = 'btn btn-sm btn-outline-secondary';
  cancel.textContent = 'cancel';
  actions.append(save, cancel);
  ed.append(note, ta, warn, actions);
  // Prose replaces the block (the editor IS its text); a request opens
  // UNDER the still-visible block, outlined so what's being asked about
  // stays on screen (round-14 drill, thread 82).
  blockEl.classList.add(prose ? 'sv-editing' : 'sv-editing-request');
  blockEl.appendChild(ed);
  let fromHash = src.hash;
  // The cursor lands at the bit that was clicked (thread 63): find the
  // clicked text in the source, since rendered text ≈ its markdown.
  ta.focus({ preventScroll: true });
  if (prose && at) {
    const idx = src.body.indexOf(at.slice(0, 80));
    if (idx >= 0) ta.setSelectionRange(idx, idx);
  }
  // A tall rendered block swaps for a shorter textarea and the viewport
  // stays put, leaving the editor's top off-screen above (round-15 drill,
  // thread 86): bring it into view unless it already is.
  const r = ed.getBoundingClientRect();
  if (r.top < 60 || r.top > innerHeight - 160) ed.scrollIntoView({ block: 'start' });
  cancel.addEventListener('click', () => closeEditor(blockEl));
  save.addEventListener('click', async () => {
    if (!ta.value.trim() && !prose) {
      warn.textContent = 'describe the change first';
      warn.hidden = false;
      return;
    }
    save.disabled = true;
    try {
      if (prose) {
        const res = await fetch('/api/edit', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            page: state.selected,
            block: blockEl.dataset.block,
            from_hash: fromHash,
            body: ta.value,
          }),
        });
        if (res.status === 409) {
          const fresh = await res.json();
          fromHash = fresh.hash;
          warn.textContent =
            'the agent changed this block while you edited — your text is kept; ' +
            'save again to overwrite theirs, or cancel to see their version';
          warn.hidden = false;
          save.disabled = false;
          return;
        }
        if (!res.ok) throw new Error(await res.text());
        closeEditor(blockEl); // the SSE patch brings the new rendering
      } else {
        // The quote is the text that was selected — the card reads "about
        // this bit", like any comment; kind (not the quote) marks it an
        // edit request. Block-id fallback only when nothing was selected.
        await postComment({
          page: state.selected,
          target: blockEl.dataset.block,
          anchor: '',
          quote: (at || '').trim().slice(0, 300) || 'edit ' + blockEl.dataset.block,
          context: null,
          body: '```md\n' + ta.value + '\n```',
          kind: 'edit',
        });
        closeEditor(blockEl); // lands in the bar; the agent merges via watch
      }
    } catch (err) {
      warn.textContent = 'save failed: ' + err.message;
      warn.hidden = false;
      save.disabled = false;
    }
  });
}

function closeEditor(blockEl) {
  blockEl.querySelector('.sv-editor')?.remove();
  blockEl.classList.remove('sv-editing', 'sv-editing-request');
  const held = pendingBlockEv.get(blockEl.dataset.block);
  if (held) {
    pendingBlockEv.delete(blockEl.dataset.block);
    applyBlock(held);
  }
}

export { startEdit, closeEditor, pendingBlockEv };


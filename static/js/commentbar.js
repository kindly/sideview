// The comment bar: the Vue island. Degrades off if the import fails —
// everything else keeps working.
import { state } from './state.js';
import { $bar } from './dom.js';
import { resolveAnchor } from './anchors.js';
import { applyCbarPref, syncConversation, postComment, setResolution } from './conversation.js';

export let vue = null;   // the vendored ESM module, once loaded
export let svc = null;   // reactive conversation store, once mounted

import('/assets/vendor/vue.esm-browser.prod.js')
  .then((m) => { vue = m; mountCommentBar(); syncConversation(); })
  .catch((e) => console.warn('sideview: comment bar disabled (vue failed to load)', e));

// ---- the bar itself: the first Vue island ------------------------------------

function mountCommentBar() {
  const { createApp, reactive, computed, watchEffect, nextTick } = vue;
  svc = reactive({ page: null, threads: [], comments: [], attachments: [], attach: {}, drafts: [] });

  // Layout classes live on body so the CSS grid can breathe around the bar.
  // Open/closed mirrors the rail: the viewer's fold is remembered per page;
  // defaults are open on wide screens, folded to the chip on small ones.
  watchEffect(() => {
    void svc.threads.length; void svc.drafts.length; void svc.page;
    applyCbarPref();
  });

  createApp({
    setup() {
      const open = computed(() => svc.threads.filter((t) => t.resolved_at == null));
      const resolved = computed(() =>
        svc.threads
          .filter((t) => t.resolved_at != null)
          .sort((a, b) => b.resolved_at - a.resolved_at) // freshest fold first
      );
      const collapsed = reactive({});
      const replies = reactive({});
      const error = reactive({ msg: '' });

      const commentsFor = (id) => svc.comments.filter((c) => c.thread_id === id);
      const lastAuthor = (id) => {
        const cs = commentsFor(id);
        return cs.length ? (cs[cs.length - 1].author || 'user') : null;
      };
      // The silence-fillers. sent: the plumbing's delivery receipt (the
      // server has it; says nothing about the agent). working: the agent's
      // own declaration for long tasks, retired by its reply.
      const sentPending = (id) => {
        const cs = commentsFor(id);
        const last = cs[cs.length - 1];
        return !!(last && last.author !== 'agent' && last.seen_at);
      };
      const fmt = (ts) =>
        new Date(ts).toLocaleString(undefined, {
          month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
        });

      const jump = (t) => {
        const el = resolveAnchor(t);
        if (!el) return;
        el.scrollIntoView({ block: 'center', behavior: 'smooth' });
        el.classList.remove('sv-flash');
        void el.offsetWidth; // restart the animation
        el.classList.add('sv-flash');
      };
      const send = async (payload, after) => {
        error.msg = '';
        try { await postComment(payload); after && after(); }
        catch (e) { error.msg = String(e.message || e); }
      };
      // ---- attachments: paste/drop into any compose box (V3.sv's plan) ----
      // Uploads happen immediately; the token controls placement, membership
      // is the chip row. Every upload attaches whether or not its token
      // survives editing.
      const replyAtts = reactive({});
      const rAtts = (id) => (replyAtts[id] || (replyAtts[id] = []));
      // Images embed, everything else links: `![a.csv](…)` is a lie in a
      // body people read and edit (found live, thread 36).
      const attToken = (a) =>
        `${a.mime.startsWith('image/') ? '!' : ''}[${a.name}](att:${a.sha256.slice(0, 8)})`;
      const uploadOne = async (file) => {
        const res = await fetch(
          '/api/attachments?name=' + encodeURIComponent(file.name || 'pasted'),
          { method: 'POST', body: file }
        );
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      };
      const composeFiles = async (e, bucket, get, set) => {
        const files = [...((e.clipboardData || e.dataTransfer)?.files || [])];
        if (!files.length) return; // plain text paste stays native
        e.preventDefault();
        const el = e.target;
        for (const f of files) {
          try {
            const a = await uploadOne(f);
            bucket.push(a);
            const text = get() || '';
            const pos =
              el && typeof el.selectionStart === 'number' ? el.selectionStart : text.length;
            set(text.slice(0, pos) + attToken(a) + text.slice(pos));
          } catch (err) {
            error.msg = String(err.message || err);
          }
        }
      };
      const draftFiles = (e, d) => composeFiles(e, d.atts, () => d.text, (v) => { d.text = v; });
      const replyFiles = (e, t) =>
        composeFiles(e, rAtts(t.id), () => replies[t.id], (v) => { replies[t.id] = v; });
      // The mobile road in: no clipboard image, no drag — the native picker.
      // A real input overlays the attach button (no scripted click()): iOS
      // opens pickers reliably only for genuine input activation — a
      // detached input did nothing and a hidden DOM one was still flaky
      // (both found live, thread 34).
      const pickChange = (e, kind, obj) => {
        const files = e.target.files;
        if (!files || !files.length) return;
        const fake = { clipboardData: { files }, preventDefault() {}, target: null };
        if (kind === 'd') draftFiles(fake, obj);
        else replyFiles(fake, obj);
        e.target.value = ''; // the same file twice still fires change
      };
      const dropAtt = (bucket, a, get, set) => {
        bucket.splice(bucket.indexOf(a), 1);
        set((get() || '').replaceAll(attToken(a), ''));
      };
      const kb = (n) => (n < 1024 ? n + ' B' : Math.max(1, Math.round(n / 1024)) + ' KB');

      // Bodies are markdown, rendered server-side (comrak, safe mode) and
      // delivered as body_html; the one client job left is resolving att:
      // image URLs against the comment's own rows. Attachments whose token
      // was edited away trail the body.
      const esc = (s) =>
        s.replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
      const fUrl = (p) => '/f/' + p.split('/').map(encodeURIComponent).join('/');
      const attHtml = (a) =>
        a.mime.startsWith('image/')
          ? `<a class="sv-att-link" href="${fUrl(a.path)}" target="_blank" rel="noopener">` +
            `<img class="sv-att-img" src="${fUrl(a.path)}" alt="${esc(a.name)}" loading="lazy"></a>`
          : `<a class="sv-att-chip" href="${fUrl(a.path)}" target="_blank" rel="noopener">` +
            `${esc(a.name)} <span>${kb(a.bytes)}</span></a>`;
      const bodyHtml = (c) => {
        const atts = svc.attachments.filter((a) => a.comment_id === c.id);
        const used = new Set();
        let html = c.body_html != null ? c.body_html : esc(c.body); // older-daemon fallback
        const swap = (m, sha) => {
          const a = atts.find((x) => x.sha256.startsWith(sha) && !used.has(x.id));
          if (!a) return m;
          used.add(a.id);
          return attHtml(a);
        };
        html = html
          .replace(/<img[^>]*\bsrc="att:([0-9a-f]{8})"[^>]*\/?>/g, swap)
          .replace(/<a[^>]*\bhref="att:([0-9a-f]{8})"[^>]*>.*?<\/a>/g, swap);
        for (const a of atts) if (!used.has(a.id)) html += attHtml(a);
        return html;
      };

      const reply = (t) => {
        const body = (replies[t.id] || '').trim();
        const atts = rAtts(t.id);
        if (!body && !atts.length) return;
        send({ thread: t.id, body: body || '(attachment)', attachments: atts }, () => {
          replies[t.id] = '';
          replyAtts[t.id] = [];
        });
      };
      const sendDraft = (d) => {
        if (!d.text.trim() && !d.atts.length) return;
        send(
          { page: d.page, target: d.target, anchor: d.anchor,
            quote: d.quote || null, context: d.context || null,
            body: d.text.trim() || '(attachment)', attachments: d.atts },
          () => { svc.drafts = svc.drafts.filter((x) => x !== d); }
        );
      };

      // Compose boxes grow with their text (field-sizing isn't in Safari
      // yet); capped so a long paste never swallows the sheet.
      const grow = (e) => {
        const el = e.target;
        el.style.height = 'auto';
        el.style.height = Math.min(el.scrollHeight + 2, 320) + 'px';
      };

      const draftsHere = computed(() => svc.drafts.filter((d) => d.page === svc.page));

      return {
        svc, draftsHere, open, resolved, collapsed, replies, error,
        commentsFor, lastAuthor, sentPending, fmt, jump, reply, sendDraft,
        rAtts, draftFiles, replyFiles, bodyHtml, pickChange, grow,
        removeDraftAtt: (d, a) => dropAtt(d.atts, a, () => d.text, (v) => { d.text = v; }),
        removeReplyAtt: (t, a) =>
          dropAtt(rAtts(t.id), a, () => replies[t.id], (v) => { replies[t.id] = v; }),
        attach: computed(() => svc.attach),
        resolve: (t) => setResolution(t.id, false),
        reopen: (t) => setResolution(t.id, true),
        toggle: (id) => { collapsed[id] = !collapsed[id]; },
        cancelDraft: (d) => { svc.drafts = svc.drafts.filter((x) => x !== d); },
        fold: () => {
          document.body.classList.remove('sv-cbar-open');
          localStorage.setItem('sv-cbar:' + svc.page, 'closed');
        },
      };
    },
    template: `
      <div v-if="svc.threads.length || draftsHere.length" class="sv-cbar-inner">
        <div class="sv-cbar-title">Comments
          <button type="button" class="sv-cbar-fold" aria-label="collapse comments"
                  title="collapse — the bubble brings it back" @click="fold"></button></div>
        <div class="sv-cbar-scroll">
        <div v-if="error.msg" class="sv-cbar-error">{{ error.msg }}</div>

        <div v-for="d in draftsHere" :key="d.key" class="sv-cbar-card sv-cbar-draft">
          <blockquote v-if="d.quote">{{ d.quote }}</blockquote>
          <textarea v-model="d.text" rows="4" placeholder="Comment… (paste or drop files)"
                    :data-draft="d.key" @input="grow"
                    @paste="draftFiles($event, d)"
                    @drop.prevent="draftFiles($event, d)" @dragover.prevent
                    @keydown.meta.enter="sendDraft(d)" @keydown.ctrl.enter="sendDraft(d)"
                    @keydown.esc="cancelDraft(d)"></textarea>
          <div v-if="d.atts.length" class="sv-att-row">
            <span v-for="a in d.atts" :key="a.sha256 + a.name" class="sv-att-pending">{{ a.name }}
              <button type="button" aria-label="remove attachment"
                      @click="removeDraftAtt(d, a)">×</button></span>
          </div>
          <div class="sv-cbar-actions">
            <button type="button" @click="sendDraft(d)">comment</button>
            <label class="sv-attach-btn" title="attach files — paste and drop work too">attach
              <input type="file" multiple @change="pickChange($event, 'd', d)"></label>
            <button type="button" class="sv-quiet" @click="cancelDraft(d)">cancel</button>
          </div>
        </div>

        <div v-for="t in open" :key="t.id"
             class="sv-cbar-card"
             :class="{ 'sv-turn': lastAuthor(t.id) === 'agent' }">
          <div class="sv-cbar-meta">
            <button v-if="attach[t.id]" type="button" class="sv-jump"
                    title="jump to the spot" @click="jump(t)">↩ {{ t.target }}</button>
            <span v-else class="sv-gone"
                  title="its anchor left the page — likely addressed">§ changed</span>
            <button type="button" class="sv-twist-btn"
                    :aria-expanded="String(!collapsed[t.id])"
                    @click="toggle(t.id)">{{ collapsed[t.id] ? '▸' : '▾' }}</button>
          </div>
          <template v-if="!collapsed[t.id]">
            <blockquote v-if="t.quote">{{ t.quote }}</blockquote>
            <div v-for="c in commentsFor(t.id)" :key="c.id" class="sv-comment">
              <span class="sv-comment-meta" :class="{ 'sv-agent': c.author === 'agent' }">
                {{ c.author || 'user' }} · {{ fmt(c.created_at) }}</span>
              <div class="sv-comment-body" v-html="bodyHtml(c)"></div>
            </div>
            <div v-if="t.working_at" class="sv-status sv-working"
                 title="the agent marked this as in progress">working…</div>
            <div v-else-if="sentPending(t.id)" class="sv-status"
                 title="delivered — the server has it; an agent hasn't necessarily read it yet">sent</div>
            <textarea v-model="replies[t.id]" rows="3" placeholder="Reply… (paste or drop files)"
                      @input="grow"
                      @paste="replyFiles($event, t)"
                      @drop.prevent="replyFiles($event, t)" @dragover.prevent
                      @keydown.meta.enter="reply(t)" @keydown.ctrl.enter="reply(t)"></textarea>
            <div v-if="rAtts(t.id).length" class="sv-att-row">
              <span v-for="a in rAtts(t.id)" :key="a.sha256 + a.name" class="sv-att-pending">{{ a.name }}
                <button type="button" aria-label="remove attachment"
                        @click="removeReplyAtt(t, a)">×</button></span>
            </div>
            <div class="sv-cbar-actions">
              <button type="button" @click="reply(t)">reply</button>
              <label class="sv-attach-btn" title="attach files — paste and drop work too">attach
                <input type="file" multiple @change="pickChange($event, 'r', t)"></label>
              <button type="button" class="sv-quiet" @click="resolve(t)"
                      title="resolve — reopenable below">resolve</button>
            </div>
          </template>
        </div>

        <details v-if="resolved.length" class="sv-cbar-resolved">
          <summary>resolved ({{ resolved.length }})</summary>
          <div v-for="t in resolved" :key="t.id" class="sv-cbar-card sv-was-resolved">
            <div class="sv-cbar-meta">
              <button v-if="attach[t.id]" type="button" class="sv-jump" @click="jump(t)">↩ {{ t.target }}</button>
              <span v-else class="sv-gone">§ changed</span>
            </div>
            <blockquote v-if="t.quote">{{ t.quote }}</blockquote>
            <div v-for="c in commentsFor(t.id)" :key="c.id" class="sv-comment">
              <span class="sv-comment-meta" :class="{ 'sv-agent': c.author === 'agent' }">
                {{ c.author || 'user' }} · {{ fmt(c.created_at) }}</span>
              <div class="sv-comment-body" v-html="bodyHtml(c)"></div>
            </div>
            <div class="sv-cbar-actions">
              <button type="button" class="sv-quiet" @click="reopen(t)">reopen</button>
            </div>
          </div>
        </details>
        </div>
      </div>
    `,
  }).mount($bar);

}


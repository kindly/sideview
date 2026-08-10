<sv-page label="Vue island" category="examples" order="20">

<sv-prose id="intro">
# An html block as a Vue island

Everything below the fold is one `sv-html` block: a whole document in a sandboxed
iframe, importing the **vendored** Vue with the bare specifier `vue`. No CDN, no
build step, no in-browser JSX transpile — the page ships Vue as a file and an
import map points the name at it.

```html
<script type="module">
  import { createApp, ref, computed } from 'vue'
  createApp({ setup() { … } }).mount('#app')
</script>
```

Three things the block gets for free, none of which it asks for: the page's
**Bootstrap and design system** (so it looks like the rest of the document
without a line of CSS), the **theme** — flip the header's ◐ and the island
follows — and **autosizing**, because the iframe reports its own height and the
frame grows to fit.

What it does *not* get is access to the page: an island is
`sandbox="allow-scripts"` on an opaque origin. It cannot read the document that
hosts it, and the only channel out is the versioned envelope
(`{sv: 1, type: "size" | "theme"}`).
</sv-prose>

<sv-html id="island">
<div id="app" class="p-3"></div>
<script type="module">
  import { createApp, ref, computed } from 'vue'

  createApp({
    setup() {
      const rows = ref(4000)
      const dropRate = ref(4)
      const dropped = computed(() => Math.round((rows.value * dropRate.value) / 100))
      const kept = computed(() => rows.value - dropped.value)
      return { rows, dropRate, dropped, kept }
    },
    template: `
      <div class="row g-4 align-items-end">
        <div class="col-sm-6">
          <label class="form-label" :for="'rows'">Rows in the file</label>
          <input id="rows" class="form-range" type="range" min="0" max="20000"
                 step="500" v-model.number="rows">
          <div class="sv-metric">{{ rows.toLocaleString() }}</div>
        </div>
        <div class="col-sm-6">
          <label class="form-label" for="rate">Blank unit rows</label>
          <input id="rate" class="form-range" type="range" min="0" max="25"
                 step="1" v-model.number="dropRate">
          <div class="sv-metric">{{ dropRate }}%</div>
        </div>
      </div>

      <table class="table mt-4 mb-0">
        <thead><tr><th>Outcome</th><th class="text-end">Rows</th></tr></thead>
        <tbody>
          <tr><td>Parsed</td><td class="text-end">{{ kept.toLocaleString() }}</td></tr>
          <tr><td>Dropped</td><td class="text-end">{{ dropped.toLocaleString() }}</td></tr>
        </tbody>
      </table>

      <div v-if="dropped > 500" class="alert alert-warning mt-3 mb-0">
        {{ dropped.toLocaleString() }} rows would be dropped — worth a second look
        before this ships.
      </div>
      <p v-else class="text-muted mt-3 mb-0">Losses are within the usual noise.</p>
    `,
  }).mount('#app')
</script>
</sv-html>

<sv-prose id="when">
## When an island is the right answer

- **A number the reader should push on.** A slider that recomputes beats three
  paragraphs of "if the rate were 8%…" — the argument becomes something the
  reader tests rather than takes.
- **Anything a static block would have to enumerate.** Options, before/after
  pairs, a matrix the reader wants to sort.

And when it is not: prose, tables and diffs are cheaper to write, cheaper to
read and comment on, and they survive in a snapshot. An island's state does not.
Reach for one when interaction *is* the point, not to decorate.
</sv-prose>

</sv-page>

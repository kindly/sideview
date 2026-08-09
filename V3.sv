<sv-page label="v3 — the project moves in">

<sv-prose id="intro">
# v3 — the project moves in

v2 closed its own loop and shipped as 0.2.0 (2026-08-09). v3 was curated the same day, through the machinery v2 built: everything ever discussed sits in the pool (IDEAS.sv); what earned its place is the In list below. **Scope settled by the author, 2026-08-09 (thread 33: "that is enough for v3").** Rationale stays in the design docs; this page holds only what v3 commits to.

Carried principles, not up for re-decision: sv files are canon, the db holds what should not be versioned · the page file has one author, everything multi-writer goes through SQLite · reference, never embed · dogfood first.
</sv-prose>

<sv-prose id="in">
## In

- **Comment attachments** — paste/drop **any file** into the compose card, images first-class (author widened it from images, 2026-08-09); the daemon writes a project file, the comment references it, the agent reads it with its own tools. Design in the attachments section below. Arrives with a **comment-card redesign**: cards sized to carry media, not just sentences. *(pulled 2026-08-09; rationale in V2.sv's candidates)*
- **Resizable side rails** — drag-to-resize the contents rail and the comment bar, widths remembered as viewer preference; the expandable bar the image cards need, so it ships with them. *(pulled 2026-08-09, thread 28)*
- **A data table block — the plan first** — the mechanism is deliberately open: a native grid (VisActor VTable leads, AntV S2 runner-up — verdicts in V2.sv's candidates) or embedding a live sqlnow session. v3 commits to the decided design, dogfooded on this page; implementation follows it. `{sql}` is reference-never-embed's first real test. *(pulled 2026-08-09, thread 29)*
- **Block plugins — the design** — how blocks from code we didn't write should work: the custom-element layer plus a small explicit `window.sideview` API, shadow-DOM isolation, framework-agnostic (HANDOFF's shape). Design-level in v3, sequenced here because the table block's sqlnow option may ride on it. *(pulled 2026-08-09, thread 30)*
- **html blocks as Vue islands — lean in** — the vendoring and CORS shipped in v2; v3 delivers the rest of the prize: skill guidance and worked examples so any agent writes artifact-grade interactive blocks against the vendored Vue, no CDN, files-in-repo persistence, the envelope already sizing and theming them. *(pulled 2026-08-09, thread 31)*
- **Binary releases + AUR** — tag-triggered GitHub releases with prebuilt per-target archives, checksums and deb/rpm (the sqlnow shape — its release.yml is the template; sideview is pure Rust with embedded assets, so it can go simpler, and musl/static is on the table since nothing dlopens), plus an AUR package. `cargo install` stops being the only door. *(author, 2026-08-09, chat)*
- **Page management at scale** — a project with many pages outgrows the chip strip: each `<sv-page>` may declare a `category` (canon, beside `order` — for committed pages, grouping is plan-worthy so it lives in the file); the switcher groups by it; unhomed pages fall to a default category that behaves exactly like today; and `/` grows a real **homepage** — the project's pages laid out by category — where today there is only the strip. Dogfood: this project's design docs convert to .sv and live in it. *(pulled 2026-08-09, thread 32)*
</sv-prose>

<sv-prose id="goal">
## Comment attachments — the little plan

- **Any file, not just images** (author, 2026-08-09): the mechanics never cared — paste or drop into the compose card, the daemon writes the file, the comment references it. Images are the first-class case: thumbnail in the card, full size on click. Everything else wears a file chip — name, size, type — opening through `/f/`.
- **Three homes, split by lifetime — the placement principle applied to files** (author, 2026-08-09). *Comment* attachments live in `.sideview/attachments/<sha8>/<original-name>` — project-local (the sandbox law), self-gitignored, conversation's lifetime: if the db goes, they are fair game. A *throwaway page's* images live beside their page, `.sideview/pages/<session>/` — the page's lifetime: `page rm` takes the directory with the file, `page promote` moves it into the repo and re-points references. A *committed document's* files live in the repo proper, beside what references them, git's and untouchable — nothing that happens to the db or the conversation may ever threaten them. **The attachments folder takes writes from the comment upload alone** — a page's images never land there, whatever the page's tier; "add it to the document" copies *out* of conversation storage into the page's own home, the same crossing as promote.
- **The channel**: the page POSTs the raw bytes to a new `/api/attachments` (filename in the query — no multipart machinery to parse), which writes the file and returns its path and hash; sending the comment binds them as rows in an `attachments` table. **Metadata in the db, bytes on disk** (author, 2026-08-09): the card, the gc and "what references this" stay one query, while the file stays where every agent's own tools — including vision — read it directly. Blobs-in-db considered and rejected by the author the same day: they'd bloat the store, lengthen write transactions, and put an extraction step between the agent and the file. A canceled draft's upload is just an unreferenced file; gc collects it. An additive migration, priced with 0.3. Same trust line as comments: the page authors conversation, never canon.
- **Watch events name the paths.** The agent reads an image with its own file tool — genuine vision, pull not push, the founding thesis — and any other file the same way. The skill grows a line.
- **Lifecycle** (author, 2026-08-09): deletion follows the document — `page rm` cascades threads, comments, and their attachments with them. Everything else is a deliberate verb, never a daemon habit: `sideview attachments gc` deletes only files nothing references, checked against live conversation *and* every current page's content, and reports what it took (a startup auto-sweep was considered and dropped — after the resurrection test it would silently take everything). `--resolved` widens it to attachments held only by resolved threads, for repos where they pile up; explicitly not the default — resolved conversation is folded, not gone. **gc's writ runs only inside `.sideview/attachments/`** — committed document assets are git's, untouchable by construction; and a page found referencing conversation storage is protected but *reported* as mis-homed ("promote this file"), because the page-content check is a backstop for that mistake, not a place to keep canon.
- **Limits, honest and few**: a size cap, a sanitized filename, a sniffed content type. Nothing executes; `/f/`'s root confinement and its `.sideview` exclusions already apply.
- **Build order** (author, 2026-08-09): backend first — table, upload, gc, watch payloads — then the comment UI rebuilt around them; the card redesign and the resizable bar land as that one pass.
- **Inline placement, plain textarea** — paste/drop inserts a markdown-shaped token at the cursor (`![name](att:<sha8>)`); the card renders the thumbnail where the token sits; the watch body keeps the token, so the agent knows *which* image "this one" is when several ride one comment. Multiple attachments per comment stay allowed — a one-attachment cap was considered and declined (it scatters one thought across cards to solve an ambiguity the tokens already solve). No contentEditable, no rich-text editor.
- **Vim motions in the compose box** — an option, off by default, a viewer preference. The credible route is CodeMirror 6 vendored as a maintainer-built bundle (rung-1 precedent: maintainer-time toolchain, one-binary promise intact), vim keymap lazy-loaded. Priced honestly at a few hundred KB of vendor weight; decided at the *end* of the compose rebuild, when the textarea's limits are felt rather than guessed — it's an additive swap, not a foundation.
- Ships with the comment-card redesign and the resizable bar, per the In list.

**Migration v3** — the model, for review before a line is written:

```sql
CREATE TABLE attachments (
    id         INTEGER PRIMARY KEY,
    comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
                                  -- rows are born when the comment is sent, never
                                  -- at upload: a canceled draft leaves only an
                                  -- unreferenced file for gc, not a dangling row
    path       TEXT NOT NULL,     -- project-relative: .sideview/attachments/<sha8>/<name>
    name       TEXT NOT NULL,     -- original filename, sanitized; kept for the agent's sake
    mime       TEXT NOT NULL,     -- sniffed at upload, never trusted from the client
    bytes      INTEGER NOT NULL,  -- the cap is enforced at upload, recorded here
    sha256     TEXT NOT NULL,     -- integrity and the dedupe key: same bytes, one file
    created_at INTEGER NOT NULL
);
CREATE INDEX attachments_by_comment ON attachments(comment_id);
CREATE INDEX attachments_by_sha    ON attachments(sha256);
-- the FK cascade clears rows when conversation goes (page rm); the daemon deletes
-- the files it queried first — SQL cannot unlink from disk, and gc backstops it.
-- Watch events carry the whole row (path, name, mime, bytes), not just a path.
```

## Goal — blessed by the author, 2026-08-09 (thread 33)

v3 is done when every item on the In list above is accepted by the author — through this page's own machinery — and **0.3.0 is on crates.io**.
</sv-prose>

</sv-page>

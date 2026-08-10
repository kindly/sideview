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
- **Three page formats, and a repo config** — `.md` and `.html` become page formats sideview accepts and tracks, not new block types (author, 2026-08-10). Design in the formats section below; it replaces the md→sv conversion, so the docs stay markdown and GitHub keeps rendering them.
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

## Three page formats, and `.sideview.toml`

- **Formats, not block types** (author, 2026-08-10). A page is a file; sideview knows three. `.sv` stays the *composed* format — typed blocks, interleaved diffs and (later) tables. `.md` and `.html` are *imported* formats: rendered whole, one block per file, through the paths that already exist. No format change, no new tag, no new thing for the skill to teach — the limitations attach to the format, where a reader looks for them.
- **What you get, stated plainly.** Markdown and inline HTML render into the page and are **fully commentable** — anchors are heading ids and paragraph hashes computed from the *rendered* text, which never cared where the text came from. A full HTML document that wants its own world is iframed instead, and there commenting stops at the block: the page cannot reach into a sandboxed frame (V0's rule, unchanged). Inline HTML runs its own CSS and JS in our page — the same trust story as markup blocks, and the same caveat: body-level layout will fight the chrome.
- **One block per file, deliberately.** Splitting a doc at its headings would give surgical patches but make block ids heading-derived — and block ids are what comments target, so renaming a heading would orphan every thread under it. One block keeps paragraph anchors surviving everything except an edit to their own paragraph; the whole-file re-render is absorbed by the reading anchor today and fixed properly by patch-in-place morphing later.
- **`.sideview.toml` at the repo root — TOML, committed, agent-authored** (author, 2026-08-10; no CLI writer, since agents edit files trivially and the one-author rule stays intact). Root, not inside `.sideview/`: that directory self-ignores with a single `*` and holds disposable machinery, while config is versioned intent — the placement principle, applied to a config file.
- **It supplies only what a file cannot say about itself.** An `.sv` page declares its own label, order and category in canon. Markdown and HTML have nowhere to put attributes, so config is their only voice: `[[pages]]` with `path`, `category`, optional `label`/`order`, and `render = "inline" | "iframe"` for HTML. Precedence follows: the file wins for anything it can express, config fills the rest.
- **Config is what survives the db.** Entries are re-applied at startup, so the resurrection test brings imported pages back too — `.sv` files announce themselves in the scan, markdown never does, and sideview must not colonize a repo's markdown by scanning for it. `port` joins it for the same reason: V2's own sign-off recorded that the remembered port dies with the db, and durability of that kind belongs in configuration.
- **Deletion splits by tier, not by format** (author, 2026-08-10, sharpening it): a throwaway page under `.sideview/pages/` is sideview's own scratch and `rm` means rm; **anything in the repo — a promoted `.sv` as much as an imported `DESIGN.md` — is unbound and left on disk**, because it is a committed file and should be exactly as scary to delete as any other. The page's ✕ never deletes canon; `page rm <id> --file` is the deliberate second word, and git is the other answer.
- **A bad config never costs you the page**: log it, report it in `status`, serve without it. And note for the docs: a worktree's store resolves to the main checkout, so the daemon reads that copy of the config.

## The comment UI around attachments

- **Mobile: the open bar is the page** (author, 2026-08-09, from the phone: the bar is already as wide as it can realistically be — so stop pretending). Two states, folded-to-chip and full-screen sheet: the sheet covers the header and chip menu, the document beneath stops being drawn, the body is locked at its reading position, and a thumb-sized × in the title row is the way out. **Built and lived in the same day** — the seven platform causes it took are in HANDOFF, and its shape is now structural (title row above a scrolling sibling, sheet pinned to the *visual* viewport so the keyboard never buries the compose box). Desktop keeps the side bar, and drag-to-resize arrives with it.
- **Compose**: paste or drop into any compose box — draft or reply — uploads immediately and inserts the token at the cursor (`![name](att:<sha8>)`); a chip row under the box shows what rides the comment, with remove. Every upload attaches whether or not its token survives editing; tokens control *placement*, not membership.
- **Cards**: image tokens render as thumbnails where they sit, tap for full size through `/f/`; non-image attachments wear a file chip — name, size — tap to open. Attachments without a token trail the body.
- **Bodies graduated to markdown on day one** (author, 2026-08-09 — raw tokens sitting in sent cards made the case the plan predicted): rendered server-side by the same comrak pass as everything, in *safe* mode — raw HTML escaped, never omitted, because a comment comes from whoever can see the page — with `att:` images resolving against the comment's own rows. The compose box stays raw text, preview-on-send; inline WYSIWYG remains the CodeMirror question, decided at the end of this pass.

## Goal — blessed by the author, 2026-08-09 (thread 33)

v3 is done when every item on the In list above is accepted by the author — through this page's own machinery — and **0.3.0 is on crates.io**.
</sv-prose>

</sv-page>

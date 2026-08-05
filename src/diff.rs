//! `sv-diff` rendering: unified diff text in, both views out.
//!
//! diffy parses the patch (git's extended format included); `similar` marks
//! word-level changes within paired lines; the aligned model, the HTML and
//! the duotone treatment are ours (V1.md's diff section — diff2html was
//! rejected there with reasons; don't re-import it).
//!
//! Both views render from one aligned model into one HTML string, the
//! inactive one hidden by CSS: `view="split|unified"` on the block is the
//! agent's default, the client's toggle is the viewer's override, and
//! toggling is instant because nothing round-trips. `render()` never errors —
//! a body that isn't a parseable diff is shown raw with an honest note.

use std::fmt::Write as _;

use diffy::patch_set::{FileOperation, ParseOptions, PatchSet};

use crate::render::text_escape;

/// Below this `similar::ratio`, a removed/added pair is treated as unrelated
/// lines rather than an edit — emphasis on unrelated pairs is noise.
const INTRALINE_THRESHOLD: f32 = 0.4;

/// One file's worth of rendered diff plus the title the outline shows.
struct FileView {
    title: String,
    body_html: String, // both tables, or the binary/empty notice
}

/// A run of removed lines aligned against a run of added lines — the model
/// both views render from. Lines are pre-escaped HTML (with `<del>`/`<ins>`
/// emphasis when the pair is similar enough to be an edit).
enum Segment {
    Context(Vec<(usize, usize, String)>),
    Change { old: Vec<(usize, String)>, new: Vec<(usize, String)> },
}

pub fn render(block_id: &str, body: &str, view: &str) -> String {
    let view = if view == "split" { "split" } else { "unified" };
    let (files, note) = parse_files(block_id, body);
    if files.is_empty() {
        // Not a diff at all: honest, raw, styled as degraded.
        return format!(
            concat!(
                r#"<p class="sv-degraded-note">{}</p>"#,
                r#"<pre><code>{}</code></pre>"#
            ),
            text_escape(note.as_deref().unwrap_or("not a unified diff — shown raw")),
            text_escape(body)
        );
    }
    let mut out = format!(
        concat!(
            r#"<figure class="sv-diff" data-view="{view}">"#,
            r#"<div class="sv-diff-bar">"#,
            r#"<button type="button" class="sv-diff-toggle" "#,
            r#"title="toggle unified / side-by-side">⇄ view</button></div>"#
        ),
        view = view
    );
    for f in &files {
        out.push_str(&f.body_html);
    }
    if let Some(note) = note {
        let _ = write!(
            out,
            r#"<div class="sv-parse-warning">{}</div>"#,
            text_escape(&note)
        );
    }
    out.push_str("</figure>");
    out
}

/// The outline contract: each file's path is a section heading, so the rail
/// navigates a multi-file diff. Anchors match the ids `render` emits.
pub fn outline(block_id: &str, body: &str) -> Vec<crate::render::Heading> {
    let (files, _) = parse_files(block_id, body);
    files
        .iter()
        .enumerate()
        .map(|(i, f)| crate::render::Heading {
            level: 2,
            text: f.title.clone(),
            id: Some(anchor(block_id, i)),
        })
        .collect()
}

fn anchor(block_id: &str, i: usize) -> String {
    format!("{block_id}-file{i}")
}

/// Parse and render each file. Tolerant by construction: files that parsed
/// before an error still render, and the error becomes a visible note.
fn parse_files(block_id: &str, body: &str) -> (Vec<FileView>, Option<String>) {
    let mut files = Vec::new();
    let mut note = None;
    for item in PatchSet::parse(body, ParseOptions::gitdiff()) {
        match item {
            Ok(fp) => {
                let title = title_for(&fp.operation().strip_prefix(1));
                let i = files.len();
                let head = format!(
                    r#"<div class="sv-diff-path" id="{}">{}</div>"#,
                    anchor(block_id, i),
                    text_escape(&title)
                );
                let body_html = match fp.patch().as_text() {
                    Some(patch) => format!("{head}{}", tables(patch)),
                    None => format!(
                        r#"{head}<div class="sv-diff-note">binary file changed</div>"#
                    ),
                };
                files.push(FileView { title, body_html });
            }
            Err(e) => {
                // The iterator can't recover past a malformed entry; say so
                // rather than silently truncating the diff.
                note = Some(if files.is_empty() {
                    format!("could not parse as a diff: {e}")
                } else {
                    format!("rest of the diff could not be parsed: {e}")
                });
                break;
            }
        }
    }
    (files, note)
}

fn title_for(op: &FileOperation<'_, str>) -> String {
    match op {
        FileOperation::Create(p) => format!("{p} (new)"),
        FileOperation::Delete(p) => format!("{p} (deleted)"),
        FileOperation::Modify { original, modified } if original == modified => {
            original.to_string()
        }
        FileOperation::Modify { original, modified } => format!("{original} → {modified}"),
        FileOperation::Rename { from, to } => format!("{from} → {to}"),
        FileOperation::Copy { from, to } => format!("{from} → {to} (copy)"),
    }
}

/// Both views of one file, from one aligned model.
fn tables(patch: &diffy::Patch<'_, str>) -> String {
    let mut unified = String::from(r#"<table class="sv-diff-table sv-view-unified"><tbody>"#);
    let mut split = String::from(r#"<table class="sv-diff-table sv-view-split"><tbody>"#);

    for hunk in patch.hunks() {
        let head = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_range().start(),
            hunk.old_range().len(),
            hunk.new_range().start(),
            hunk.new_range().len()
        );
        let _ = write!(
            unified,
            r#"<tr class="sv-hunk"><td class="sv-no"></td><td class="sv-no"></td><td>{head}</td></tr>"#
        );
        let _ = write!(
            split,
            r#"<tr class="sv-hunk"><td class="sv-no"></td><td></td><td class="sv-no"></td><td>{head}</td></tr>"#
        );

        for seg in segments(hunk) {
            match seg {
                Segment::Context(lines) => {
                    for (o, n, html) in lines {
                        let _ = write!(
                            unified,
                            r#"<tr><td class="sv-no">{o}</td><td class="sv-no">{n}</td><td>{html}</td></tr>"#
                        );
                        let _ = write!(
                            split,
                            r#"<tr><td class="sv-no">{o}</td><td>{html}</td><td class="sv-no">{n}</td><td>{html}</td></tr>"#
                        );
                    }
                }
                Segment::Change { old, new } => {
                    // Unified interleaves the model: the removed run, then
                    // the added run — the order the patch itself uses.
                    for (o, html) in &old {
                        let _ = write!(
                            unified,
                            r#"<tr class="sv-del"><td class="sv-no">{o}</td><td class="sv-no"></td><td>{html}</td></tr>"#
                        );
                    }
                    for (n, html) in &new {
                        let _ = write!(
                            unified,
                            r#"<tr class="sv-ins"><td class="sv-no"></td><td class="sv-no">{n}</td><td>{html}</td></tr>"#
                        );
                    }
                    // Split is the two-column reading: pairs side by side,
                    // unpaired lines against empty cells.
                    for i in 0..old.len().max(new.len()) {
                        let (ono, ohtml) = old
                            .get(i)
                            .map(|(n, h)| (n.to_string(), h.as_str()))
                            .unwrap_or_default();
                        let (nno, nhtml) = new
                            .get(i)
                            .map(|(n, h)| (n.to_string(), h.as_str()))
                            .unwrap_or_default();
                        let oc = if old.get(i).is_some() { "sv-del" } else { "sv-empty" };
                        let nc = if new.get(i).is_some() { "sv-ins" } else { "sv-empty" };
                        let _ = write!(
                            split,
                            r#"<tr><td class="sv-no">{ono}</td><td class="{oc}">{ohtml}</td><td class="sv-no">{nno}</td><td class="{nc}">{nhtml}</td></tr>"#
                        );
                    }
                }
            }
        }
    }
    unified.push_str("</tbody></table>");
    split.push_str("</tbody></table>");
    format!("{unified}{split}")
}

/// Fold a hunk's lines into the aligned model, numbering as we go and
/// rendering intraline emphasis on paired lines.
fn segments(hunk: &diffy::Hunk<'_, str>) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut old_no = hunk.old_range().start();
    let mut new_no = hunk.new_range().start();
    let mut ctx: Vec<(usize, usize, String)> = Vec::new();
    let mut dels: Vec<usize> = Vec::new();
    let mut del_texts: Vec<&str> = Vec::new();
    let mut inss: Vec<usize> = Vec::new();
    let mut ins_texts: Vec<&str> = Vec::new();

    let flush_change = |dels: &mut Vec<usize>,
                            del_texts: &mut Vec<&str>,
                            inss: &mut Vec<usize>,
                            ins_texts: &mut Vec<&str>,
                            out: &mut Vec<Segment>| {
        if dels.is_empty() && inss.is_empty() {
            return;
        }
        let (old_html, new_html) = emphasize(del_texts, ins_texts);
        out.push(Segment::Change {
            old: dels.drain(..).zip(old_html).collect(),
            new: inss.drain(..).zip(new_html).collect(),
        });
        del_texts.clear();
        ins_texts.clear();
    };

    for line in hunk.lines() {
        match line {
            diffy::Line::Context(t) => {
                flush_change(&mut dels, &mut del_texts, &mut inss, &mut ins_texts, &mut out);
                ctx.push((old_no, new_no, text_escape(t.trim_end_matches('\n'))));
                old_no += 1;
                new_no += 1;
            }
            diffy::Line::Delete(t) => {
                if !ctx.is_empty() {
                    out.push(Segment::Context(std::mem::take(&mut ctx)));
                }
                dels.push(old_no);
                del_texts.push(t.trim_end_matches('\n'));
                old_no += 1;
            }
            diffy::Line::Insert(t) => {
                if !ctx.is_empty() {
                    out.push(Segment::Context(std::mem::take(&mut ctx)));
                }
                inss.push(new_no);
                ins_texts.push(t.trim_end_matches('\n'));
                new_no += 1;
            }
        }
    }
    flush_change(&mut dels, &mut del_texts, &mut inss, &mut ins_texts, &mut out);
    if !ctx.is_empty() {
        out.push(Segment::Context(ctx));
    }
    out
}

/// Intraline emphasis: pair the runs index-wise; a pair similar enough to be
/// an edit gets `<del>`/`<ins>` around the words that changed, gated by
/// `similar`'s ratio so unrelated pairs stay plain.
fn emphasize(old: &[&str], new: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut old_html: Vec<String> = old.iter().map(|t| text_escape(t)).collect();
    let mut new_html: Vec<String> = new.iter().map(|t| text_escape(t)).collect();
    for i in 0..old.len().min(new.len()) {
        // Gate on character similarity (word tokenization counts whitespace
        // as matches and flatters unrelated lines), emphasize by words.
        if similar::TextDiff::from_chars(old[i], new[i]).ratio() < INTRALINE_THRESHOLD {
            continue;
        }
        let diff = similar::TextDiff::from_words(old[i], new[i]);
        let mut o = String::new();
        let mut n = String::new();
        for change in diff.iter_all_changes() {
            let text = text_escape(change.value());
            match change.tag() {
                similar::ChangeTag::Equal => {
                    o.push_str(&text);
                    n.push_str(&text);
                }
                similar::ChangeTag::Delete => {
                    let _ = write!(o, "<del>{text}</del>");
                }
                similar::ChangeTag::Insert => {
                    let _ = write!(n, "<ins>{text}</ins>");
                }
            }
        }
        old_html[i] = o;
        new_html[i] = n;
    }
    (old_html, new_html)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_FILES: &str = "\
diff --git a/src/one.rs b/src/one.rs
index 111..222 100644
--- a/src/one.rs
+++ b/src/one.rs
@@ -1,3 +1,3 @@
 fn main() {
-    let x = 3;
+    let x = 4;
 }
diff --git a/docs/two.md b/docs/two.md
index 333..444 100644
--- a/docs/two.md
+++ b/docs/two.md
@@ -1,2 +1,2 @@
-old heading
+new heading
 body
";

    #[test]
    fn multi_file_diffs_render_both_views_with_line_numbers() {
        let html = render("b1", TWO_FILES, "unified");
        assert!(html.contains("src/one.rs"), "{html}");
        assert!(html.contains("docs/two.md"));
        assert!(html.contains("sv-view-unified") && html.contains("sv-view-split"));
        assert!(html.contains(r#"<td class="sv-no">2</td>"#), "line numbers: {html}");
        assert!(html.contains(r#"data-view="unified""#));
        assert!(render("b1", TWO_FILES, "split").contains(r#"data-view="split""#));
    }

    #[test]
    fn intraline_emphasis_marks_the_changed_word_only() {
        let html = render("b1", TWO_FILES, "unified");
        assert!(html.contains("<del>3;</del>"), "{html}");
        assert!(html.contains("<ins>4;</ins>"), "{html}");
        // The unchanged part of the pair is not wrapped.
        assert!(html.contains("let x = <del>"), "shared prefix stays plain: {html}");
    }

    #[test]
    fn unrelated_pairs_get_no_emphasis() {
        let (o, n) = emphasize(&["completely different content"], &["zzz qqq vvv"]);
        assert!(!o[0].contains("<del>"), "{o:?}");
        assert!(!n[0].contains("<ins>"), "{n:?}");
    }

    #[test]
    fn outline_lists_file_paths_and_anchors_match_the_html() {
        let headings = outline("b1", TWO_FILES);
        assert_eq!(
            headings.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(),
            vec!["src/one.rs", "docs/two.md"]
        );
        let html = render("b1", TWO_FILES, "unified");
        for h in &headings {
            assert_eq!(h.level, 2);
            let anchor = format!(r#"id="{}""#, h.id.as_deref().unwrap());
            assert!(html.contains(&anchor), "missing {anchor}");
        }
    }

    #[test]
    fn garbage_degrades_to_raw_mono_and_never_errors() {
        let html = render("b1", "this is <b>not</b> a diff at all", "unified");
        assert!(html.contains("sv-degraded-note"), "{html}");
        assert!(html.contains("&lt;b&gt;not&lt;/b&gt;"), "raw and escaped: {html}");
        assert!(outline("b1", "not a diff").is_empty());
    }

    #[test]
    fn renames_and_creates_are_titled_honestly() {
        let renamed = "\
diff --git a/old.rs b/new.rs
similarity index 90%
rename from old.rs
rename to new.rs
";
        let headings = outline("b1", renamed);
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].text, "old.rs → new.rs");
    }
}

//! The `.sv` block-document format: the canonical source of every page.
//!
//! This is the owned fence scanner V1.md specifies — deliberately not an HTML
//! parser (HTML cannot give raw bodies to custom tags), not YAML (streams are
//! YAML all the way down), not markdown (a prose notation, not a data format).
//! The full alternatives trail lives in V1.md's "format, stress-tested"
//! section; do not re-litigate here.
//!
//! The rules, few and uniform:
//! - A tag is recognized at column 0 only. A body that needs to show literal
//!   sv-tags indents them by one space.
//! - `<sv-NAME key="value" …>` alone on a line opens a block; the body is raw
//!   bytes — never entity-decoded, never trimmed — until `</sv-NAME>` alone on
//!   a line. Plain `<` and `&` inside a body are always correct.
//! - Blocks do not nest, which buys recovery no stock parser can offer: an
//!   opener inside a body almost certainly means the previous close tag was
//!   lost, so the scanner implicitly closes with a visible warning and every
//!   later block stays healthy. An unclosed final block runs to end-of-file
//!   with the same warning.
//! - `<sv-page …>` carries page properties (label, outline). Its closer is
//!   optional noise; nothing nests inside it semantically.
//! - Unknown `sv-` types parse fine — the renderer degrades them visibly, so
//!   a newer file meeting an older binary is honest, not silent.
//! - Stray top-level content is collected into a warning block rather than
//!   dropped: visible beats silent, and a half-written file must render as
//!   an honest error that heals on the next save.

/// One parsed block. `lines` is the half-open line range of the whole block
/// (opener through closer) in the source — the CLI edits files by splicing
/// these ranges, never by re-serializing, so untouched bytes stay untouched.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub type_name: String,
    pub attrs: Vec<(String, String)>,
    pub body: String,
    pub warnings: Vec<String>,
    pub lines: (usize, usize),
}

impl Block {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn id(&self) -> Option<&str> {
        self.attr("id")
    }
}

#[derive(Debug, Default)]
pub struct Page {
    /// From `<sv-page …>`: label, outline, whatever future keys arrive.
    pub props: Vec<(String, String)>,
    pub blocks: Vec<Block>,
    /// File-level complaints that belong to no block (a duplicate sv-page,
    /// a stray closer). Rendered as a page notice.
    pub warnings: Vec<String>,
}

impl Page {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// A recognized line at column 0. Anything else is body or stray content.
enum Marker {
    Open { name: String, attrs: Vec<(String, String)> },
    Close { name: String },
}

/// `<sv-name key="value">` → Open, `</sv-name>` → Close, everything else →
/// None. Strict on shape (the grammar is the contract) but returns None
/// rather than erroring — an almost-tag is just body text.
fn marker(line: &str) -> Option<Marker> {
    let line = line.trim_end();
    if let Some(rest) = line.strip_prefix("</sv-") {
        let name = rest.strip_suffix('>')?;
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
            return Some(Marker::Close { name: format!("sv-{name}") });
        }
        return None;
    }
    let rest = line.strip_prefix("<sv-")?;
    let rest = rest.strip_suffix('>')?;
    let mut chars = rest.char_indices().peekable();
    let mut name_end = rest.len();
    for (i, c) in chars.by_ref() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            continue;
        }
        if c == ' ' {
            name_end = i;
            break;
        }
        return None; // not a tag after all
    }
    let name = &rest[..name_end];
    if name.is_empty() {
        return None;
    }
    let attrs = parse_attrs(rest[name_end..].trim())?;
    Some(Marker::Open { name: format!("sv-{name}"), attrs })
}

/// `key="value" key2="value2"`. Double quotes only, no escapes — a value
/// cannot contain `"`, and the CLI refuses to write one that does. Returns
/// None on malformed input so the line falls back to body/stray text.
fn parse_attrs(mut s: &str) -> Option<Vec<(String, String)>> {
    let mut attrs = Vec::new();
    while !s.is_empty() {
        let eq = s.find("=\"")?;
        let key = s[..eq].trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return None;
        }
        let rest = &s[eq + 2..];
        let close = rest.find('"')?;
        attrs.push((key.to_string(), rest[..close].to_string()));
        s = rest[close + 1..].trim_start();
    }
    Some(attrs)
}

pub fn parse(source: &str) -> Page {
    let lines: Vec<&str> = source.lines().collect();
    let mut page = Page::default();
    let mut seen_page_tag = false;

    // In-progress block: (type_name, attrs, body lines, warnings, start line).
    let mut open: Option<(String, Vec<(String, String)>, Vec<&str>, Vec<String>, usize)> = None;
    // Stray top-level lines being collected: (lines, start line).
    let mut stray: Option<(Vec<&str>, usize)> = None;

    let finish =
        |page: &mut Page,
         open: &mut Option<(String, Vec<(String, String)>, Vec<&str>, Vec<String>, usize)>,
         end: usize| {
            if let Some((type_name, attrs, body, warnings, start)) = open.take() {
                page.blocks.push(Block {
                    type_name,
                    attrs,
                    body: body.join("\n"),
                    warnings,
                    lines: (start, end),
                });
            }
        };
    let flush_stray = |page: &mut Page, stray: &mut Option<(Vec<&str>, usize)>, end: usize| {
        if let Some((lines, start)) = stray.take() {
            page.blocks.push(Block {
                type_name: "sv-stray".into(),
                attrs: vec![],
                body: lines.join("\n"),
                warnings: vec![format!(
                    "content outside any block (lines {}–{}) — wrap it in an sv- tag",
                    start + 1,
                    end
                )],
                lines: (start, end),
            });
        }
    };

    for (i, raw) in lines.iter().enumerate() {
        match (marker(raw), open.is_some()) {
            (Some(Marker::Open { name, attrs }), in_block) => {
                if in_block {
                    // Blocks don't nest: the previous close tag was lost.
                    let (_, _, _, warnings, start) = open.as_mut().unwrap();
                    warnings.push(format!(
                        "block opened at line {} was never closed — closed it at line {} where `<{}>` opens",
                        *start + 1,
                        i,
                        name
                    ));
                    finish(&mut page, &mut open, i);
                }
                flush_stray(&mut page, &mut stray, i);
                if name == "sv-page" {
                    if seen_page_tag {
                        page.warnings.push(format!("duplicate <sv-page> at line {} ignored", i + 1));
                    } else {
                        seen_page_tag = true;
                        page.props = attrs;
                    }
                } else {
                    open = Some((name, attrs, Vec::new(), Vec::new(), i));
                }
            }
            (Some(Marker::Close { name }), true) => {
                let matches = open.as_ref().map(|(n, ..)| *n == name).unwrap_or(false);
                if matches {
                    finish(&mut page, &mut open, i + 1);
                } else {
                    // A closer for some other type mid-body is body text —
                    // raw means raw.
                    open.as_mut().unwrap().2.push(raw);
                }
            }
            (Some(Marker::Close { name }), false) => {
                if name != "sv-page" {
                    page.warnings
                        .push(format!("stray `</{}>` at line {} — no block is open", name, i + 1));
                }
            }
            (None, true) => open.as_mut().unwrap().2.push(raw),
            (None, false) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() || (trimmed.starts_with("<!--") && trimmed.ends_with("-->")) {
                    flush_stray(&mut page, &mut stray, i);
                } else {
                    stray.get_or_insert_with(|| (Vec::new(), i)).0.push(raw);
                }
            }
        }
    }
    if let Some((_, _, _, warnings, start)) = open.as_mut() {
        warnings.push(format!(
            "block opened at line {} was never closed — ran to end of file",
            *start + 1
        ));
    }
    finish(&mut page, &mut open, lines.len());
    flush_stray(&mut page, &mut stray, lines.len());
    page
}

/// Serialize one block. The CLI splices this into the file at a line range;
/// it never re-serializes the whole document.
pub fn block_text(type_name: &str, attrs: &[(&str, &str)], body: &str) -> String {
    let mut open = format!("<{type_name}");
    for (k, v) in attrs {
        open.push_str(&format!(" {k}=\"{v}\""));
    }
    open.push('>');
    let body = body.strip_suffix('\n').unwrap_or(body);
    format!("{open}\n{body}\n</{type_name}>")
}

/// An attribute value the grammar can hold: no double quotes (no escapes
/// exist), no newlines. The CLI bails with this message rather than writing
/// a file it couldn't re-read.
pub fn check_attr_value(v: &str) -> Result<(), String> {
    if v.contains('"') || v.contains('\n') {
        return Err("attribute values cannot contain double quotes or newlines".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_are_raw_bytes() {
        let src = "<sv-prose id=\"b1\">\nif x < 3 && y > 4 {}\n<div>not a tag here</div>\n &amp; stays literal\n</sv-prose>\n";
        let page = parse(src);
        assert_eq!(page.blocks.len(), 1);
        let b = &page.blocks[0];
        assert_eq!(b.type_name, "sv-prose");
        assert_eq!(b.id(), Some("b1"));
        assert_eq!(
            b.body,
            "if x < 3 && y > 4 {}\n<div>not a tag here</div>\n &amp; stays literal"
        );
        assert!(b.warnings.is_empty());
    }

    #[test]
    fn tags_are_recognized_at_column_zero_only() {
        // The escape hatch this plan's own page needed on day one: indented
        // tags are body text.
        let src = "<sv-prose id=\"b1\">\n <sv-prose id=\"x\">\n </sv-prose>\n</sv-prose>\n";
        let page = parse(src);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].body, " <sv-prose id=\"x\">\n </sv-prose>");
        assert!(page.blocks[0].warnings.is_empty());
    }

    #[test]
    fn a_lost_close_tag_heals_at_the_next_opener() {
        let src = "<sv-prose id=\"b1\">\nfirst\n<sv-markup id=\"b2\">\n<b>second</b>\n</sv-markup>\n";
        let page = parse(src);
        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[0].body, "first");
        assert_eq!(page.blocks[0].warnings.len(), 1, "the healed block carries the warning");
        assert!(page.blocks[1].warnings.is_empty(), "later blocks stay healthy");
    }

    #[test]
    fn unclosed_final_block_runs_to_eof_with_a_warning() {
        let page = parse("<sv-prose id=\"b1\">\nstill going");
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].body, "still going");
        assert_eq!(page.blocks[0].warnings.len(), 1);
    }

    #[test]
    fn a_mismatched_closer_is_body_text() {
        // `</sv-html>` inside a prose body — raw means raw.
        let src = "<sv-prose id=\"b1\">\n</sv-html>\n</sv-prose>\n";
        let page = parse(src);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].body, "</sv-html>");
    }

    #[test]
    fn page_props_and_stray_content_and_unknown_types() {
        let src = "<sv-page label=\"My plan\" outline=\"tabs\">\n\nloose text\n<sv-table id=\"b1\">\ncells\n</sv-table>\n</sv-page>\n";
        let page = parse(src);
        assert_eq!(page.prop("label"), Some("My plan"));
        assert_eq!(page.prop("outline"), Some("tabs"));
        // stray block first (flushed when the sv-table opener arrived)
        assert_eq!(page.blocks[0].type_name, "sv-stray");
        assert_eq!(page.blocks[0].body, "loose text");
        // unknown type parses; rendering degrades it visibly, not silently
        assert_eq!(page.blocks[1].type_name, "sv-table");
        assert!(page.warnings.is_empty(), "matched </sv-page> is not a complaint");
    }

    #[test]
    fn line_ranges_support_splicing() {
        let src = "<sv-page>\n<sv-prose id=\"b1\">\none\n</sv-prose>\n<sv-prose id=\"b2\">\ntwo\n</sv-prose>\n</sv-page>\n";
        let page = parse(src);
        let lines: Vec<&str> = src.lines().collect();
        let b2 = &page.blocks[1];
        assert_eq!(
            lines[b2.lines.0..b2.lines.1].join("\n"),
            "<sv-prose id=\"b2\">\ntwo\n</sv-prose>"
        );
    }

    #[test]
    fn block_text_round_trips() {
        let text = block_text("sv-prose", &[("id", "b7")], "hello\nworld");
        let page = parse(&text);
        assert_eq!(page.blocks[0].body, "hello\nworld");
        assert_eq!(page.blocks[0].id(), Some("b7"));
    }
}

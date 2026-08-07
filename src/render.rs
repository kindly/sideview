//! The daemon sends rendered HTML, not sources. comrak runs here, the design
//! vocabulary lives here, and the SSE payload is the finished element — so the
//! client never needs a markdown parser and cannot drift out of sync.

use crate::format::Block;

/// Render a parsed block into the element the page inserts. Never errors:
/// unknown types and parse warnings render as visibly marked content —
/// a newer file meeting an older binary degrades honestly, not silently.
pub fn block(id: &str, b: &Block) -> String {
    let (class, body) = match b.type_name.as_str() {
        // Headings get ids prefixed with the block's id, so outline entries
        // can jump to them and two blocks' "Overview" don't collide.
        "sv-prose" => ("sv-block", markdown_opts(&b.body, Some(&format!("{id}-")))),
        // Not sanitized; scripts run. A decision, not an oversight — the block
        // is code you own in a page only you can reach. See DESIGN.md.
        "sv-markup" => ("sv-block", b.body.clone()),
        "sv-html" => ("sv-block", iframe(&b.body, b.attr("height"))),
        // In-file annotation: canon, so always visible (the visibility law) —
        // margin-note styling with its target worn as a reference line.
        // Physical placement at the anchor is the client's future refinement;
        // the honest v2 form is a visible note that names its spot.
        "sv-note" => ("sv-block sv-note", note(b)),
        "sv-diff" => (
            "sv-block",
            crate::diff::render(id, &b.body, b.attr("view").unwrap_or("unified")),
        ),
        // The parser's honest container for top-level content outside any
        // block — shown raw so the author can see exactly what to fix.
        "sv-stray" => ("sv-block sv-degraded", preformatted(&b.body)),
        // TODO(v1): pass markup through a lenient HTML parser here and log
        // unknown sv-/Bootstrap classes and inline style= attributes.
        other => (
            "sv-block sv-degraded",
            format!(
                r#"<p class="sv-degraded-note">this sideview doesn't know `{}` — upgrade to show it properly</p>{}"#,
                text_escape(other),
                preformatted(&b.body)
            ),
        ),
    };
    let warnings: String = b
        .warnings
        .iter()
        .map(|w| format!(r#"<div class="sv-parse-warning">{}</div>"#, text_escape(w)))
        .collect();
    format!(
        r#"<section class="{class}" data-block="{id}" data-type="{}">{warnings}{body}</section>"#,
        text_escape(&b.type_name)
    )
}

/// An sv-note's body is markdown like prose; the target/at attributes render
/// as a quiet mono reference line above it.
fn note(b: &Block) -> String {
    let place = match (b.attr("target"), b.attr("at")) {
        (Some(t), Some(a)) => format!("{t} · {a}"),
        (Some(t), None) => t.to_string(),
        _ => String::new(),
    };
    let refline = if place.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="sv-note-ref">→ {}</div>"#, text_escape(&place))
    };
    format!("{refline}{}", markdown_opts(&b.body, None))
}

/// GFM via comrak — tables, task lists and strikethrough turn up constantly
/// in plans. `unsafe` because prose is the author's own HTML-bearing
/// markdown, same trust story as markup blocks.
fn markdown_opts(text: &str, header_id_prefix: Option<&str>) -> String {
    let mut options = comrak_options();
    options.extension.header_id_prefix = header_id_prefix.map(str::to_string);
    let mut plugins = comrak::options::Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(highlighter());
    comrak::markdown_to_html_with_plugins(text, &options, &plugins)
}

/// Syntect, in the same pass that renders the markdown — highlighting is not
/// a second system (V1.md). Class-based output, never inline styles: one
/// rendered HTML string serves both themes, and the classes get a duotone
/// treatment in sideview.css. Built once: loading the syntax set costs real
/// milliseconds and the daemon renders on every poll tick.
///
/// A ```mermaid fence passes through unharmed — syntect wraps its text in
/// spans, but the client reads `textContent` of `code.language-mermaid`, and
/// comrak keeps that class on the code tag regardless of the highlighter.
fn highlighter() -> &'static comrak::plugins::syntect::SyntectAdapter {
    static ADAPTER: std::sync::OnceLock<comrak::plugins::syntect::SyntectAdapter> =
        std::sync::OnceLock::new();
    ADAPTER.get_or_init(|| {
        comrak::plugins::syntect::SyntectAdapterBuilder::new()
            .css_with_class_prefix("sv-")
            .build()
    })
}

fn comrak_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.render.r#unsafe = true;
    options
}

fn preformatted(body: &str) -> String {
    format!("<pre><code>{}</code></pre>", text_escape(body))
}

/// One heading a block contains, as reported to the page for its outline.
/// `id` is an anchor reachable from the page: comrak-generated for prose,
/// author-supplied (if any) for markup, never for iframe-isolated html.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// The outline a block supplies: derived from its content at event time, the
/// same way its HTML is — never stored, so improving extraction never needs a
/// backfill. A future explicit `headings` attribute is the escape hatch for
/// types that defy autodetection.
pub fn outline(id: &str, b: &Block) -> Vec<Heading> {
    match b.type_name.as_str() {
        // Prose headings ARE document structure, in the canonical file —
        // deriving the rail from them is reading canon, not inference.
        "sv-prose" => prose_outline(&b.body, &format!("{id}-")),
        // Markup headings are presentation (a label on a card is not a
        // section): inferring structure from them broke the rail twice —
        // multi-heading blocks, then a pi run putting h-tags in a card row —
        // and the author pulled it on 2026-08-06. Markup and html blocks
        // contribute nothing until the explicit agent-supplied outline
        // (V1.md, v2) gives them a way to say what they mean.
        "sv-markup" | "sv-html" => Vec::new(),
        // File paths as sections — the rail navigates a multi-file diff.
        "sv-diff" => crate::diff::outline(id, &b.body),
        _ => Vec::new(),
    }
}

fn prose_outline(text: &str, prefix: &str) -> Vec<Heading> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, text, &comrak_options());
    // A second Anchorizer fed the same texts in the same order produces the
    // same slugs as the one inside comrak's renderer — that determinism is
    // what keeps these ids honest.
    let mut anchorizer = comrak::Anchorizer::new();
    let mut out = Vec::new();
    for node in root.descendants() {
        let level = match &node.data.borrow().value {
            comrak::nodes::NodeValue::Heading(h) => Some(h.level),
            _ => None,
        };
        let Some(level) = level else { continue };
        let text: String = node
            .descendants()
            .skip(1)
            .filter_map(|n| match &n.data.borrow().value {
                comrak::nodes::NodeValue::Text(t) => Some(t.to_string()),
                comrak::nodes::NodeValue::Code(c) => Some(c.literal.to_string()),
                _ => None,
            })
            .collect();
        let id = format!("{prefix}{}", anchorizer.anchorize(&text));
        out.push(Heading { level, text, id: Some(id) });
    }
    out
}

/// Lenient parse — never errors, whatever an agent wrote. `keep_ids` is true
/// for markup (an author-supplied id is reachable in the page, which makes it
/// the explicit supply mechanism) and false for iframe-isolated documents.
fn fragment_outline(html: &str, keep_ids: bool) -> Vec<Heading> {
    let doc = scraper::Html::parse_fragment(html);
    let selector =
        scraper::Selector::parse("h1,h2,h3,h4,h5,h6").expect("static selector parses");
    doc.select(&selector)
        .map(|el| Heading {
            level: el.value().name().as_bytes()[1] - b'0',
            text: el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "),
            id: if keep_ids { el.value().id().map(str::to_string) } else { None },
        })
        .collect()
}

/// Whole documents are isolated in a sandboxed iframe, with the same base
/// stylesheets injected so isolated blocks look consistent for free — plus
/// the envelope script: a versioned postMessage channel ({sv:1, type:…})
/// that carries size out (ResizeObserver, retiring the 85vh interim) and
/// theme in (the override finally reaches srcdoc, which can't see our
/// localStorage). One channel because theme, React blocks and origin-iframed
/// service blocks all want it — see V2.sv.
fn iframe(document: &str, height: Option<&str>) -> String {
    // A const rather than an inline format string: the envelope script is
    // full of JS braces, which format! would try to parse.
    const PRELUDE: &str = concat!(
        r#"<link rel="stylesheet" href="/assets/vendor/bootstrap.min.css">"#,
        r#"<link rel="stylesheet" href="/assets/sideview.css">"#,
        "<script>",
        // The OS guess paints first; the parent's theme message corrects
        // it as soon as the size handshake announces this iframe.
        r#"document.documentElement.setAttribute('data-bs-theme',"#,
        r#"matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light');"#,
        r#"addEventListener('message',(e)=>{const m=e.data;"#,
        r#"if(m&&m.sv===1&&m.type==='theme')document.documentElement.setAttribute('data-bs-theme',m.mode)});"#,
        r#"(()=>{const report=()=>parent.postMessage({sv:1,type:'size',"#,
        r#"height:document.documentElement.scrollHeight},'*');"#,
        r#"const ready=()=>{report();const ro=new ResizeObserver(report);"#,
        r#"ro.observe(document.documentElement);if(document.body)ro.observe(document.body)};"#,
        r#"if(document.readyState==='complete')ready();else addEventListener('load',ready)})()"#,
        "</script>",
    );
    let with_style = format!("{PRELUDE}{document}");
    let srcdoc = attr_escape(&with_style);
    // An explicit `height` attribute is the agent saying "this tall": it
    // pins the iframe (data-sv-fixed tells the parent to ignore size
    // reports), and the viewer's drag still wins over both.
    let style = match height.filter(|h| is_css_length(h)) {
        Some(h) => format!(r#" data-sv-fixed="1" style="height:{h}""#),
        None => String::new(),
    };
    format!(r#"<iframe class="sv-html" sandbox="allow-scripts"{style} srcdoc="{srcdoc}"></iframe>"#)
}

/// A plausible CSS length (`40rem`, `600px`, `85vh`, `50%`) and nothing more —
/// not because the author is distrusted (markup blocks are unsanitized by
/// design), but because a typo'd unit should fall back to the default rather
/// than emit a broken style attribute.
fn is_css_length(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 16
        && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '%')
        && h.starts_with(|c: char| c.is_ascii_digit())
}

fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

pub(crate) fn text_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    fn parse_one(src: &str) -> Block {
        let page = format::parse(src);
        assert_eq!(page.blocks.len(), 1, "test fixture must be one block");
        page.blocks.into_iter().next().unwrap()
    }

    #[test]
    fn code_fences_highlight_with_prefixed_classes_and_no_inline_styles() {
        let b = parse_one(
            "<sv-prose id=\"b1\">\n```rust\nfn main() { let x = \"hi\"; }\n```\n</sv-prose>",
        );
        let html = block("b1", &b);
        assert!(html.contains("sv-keyword"), "keywords get classed: {html}");
        assert!(html.contains("sv-string"), "strings get classed: {html}");
        assert!(
            !html.contains("style=\"background"),
            "class mode, never a baked-in theme: {html}"
        );
    }

    #[test]
    fn mermaid_fences_keep_their_client_side_contract() {
        let b = parse_one(
            "<sv-prose id=\"b2\">\n```mermaid\ngraph LR\nA --> B\n```\n</sv-prose>",
        );
        let html = block("b2", &b);
        assert!(
            html.contains(r#"class="language-mermaid""#),
            "the client's selector is `code.language-mermaid`: {html}"
        );
    }

    #[test]
    fn prose_renders_gfm_tables() {
        let b = parse_one("<sv-prose id=\"b1\">\n| a |\n|---|\n| 1 |\n</sv-prose>");
        let html = block("b1", &b);
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains(r#"data-block="b1""#));
    }

    #[test]
    fn unknown_type_degrades_visibly_with_its_content() {
        let b = parse_one("<sv-table id=\"b2\">\nselect 1\n</sv-table>");
        let html = block("b2", &b);
        assert!(html.contains("doesn't know"), "{html}");
        assert!(html.contains("select 1"), "the raw body stays visible: {html}");
    }


    #[test]
    fn code_fences_do_not_double_escape() {
        let b = parse_one("<sv-prose id=\"b9\">\n```text\nsideview comment <block>\n```\n</sv-prose>");
        let html = block("b9", &b);
        assert!(!html.contains("&amp;lt;"), "double escaped: {html}");
        assert!(html.contains("&lt;block&gt;"), "single escape expected: {html}");
    }

    #[test]
    fn sv_note_is_canon_visible_and_wears_its_target() {
        let b = parse_one("<sv-note target=\"b3\" at=\"p:3f9c2a1b04d2\">\n**Own** the anchor.\n</sv-note>");
        let html = block("n1", &b);
        assert!(html.contains("sv-note"), "{html}");
        assert!(html.contains("b3 · p:3f9c2a1b04d2"), "the reference line names the spot: {html}");
        assert!(html.contains("<strong>Own</strong>"), "the body is markdown: {html}");
    }

    #[test]
    fn parse_warnings_are_shown_in_the_block() {
        let page = format::parse("<sv-prose id=\"b1\">\nfirst\n<sv-prose id=\"b2\">\nsecond\n</sv-prose>");
        let html = block("b1", &page.blocks[0]);
        assert!(html.contains("sv-parse-warning"), "{html}");
        assert!(html.contains("never closed"), "{html}");
    }

    #[test]
    fn prose_outline_ids_match_rendered_anchors() {
        let b = parse_one(
            "<sv-prose id=\"b3\">\n## Alpha\n\ntext\n\n### Beta `code`\n\n## Alpha\n</sv-prose>",
        );
        let headings = outline("b3", &b);
        assert_eq!(
            headings.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(),
            vec![(2, "Alpha"), (3, "Beta code"), (2, "Alpha")]
        );
        // Every extracted id must exist in the rendered HTML, dedup included.
        let html = block("b3", &b);
        for h in &headings {
            let anchor = format!(r#"id="{}""#, h.id.as_deref().unwrap());
            assert!(html.contains(&anchor), "missing {anchor} in {html}");
        }
        assert_ne!(headings[0].id, headings[2].id, "duplicate titles must dedup");
    }

    #[test]
    fn markup_contributes_no_outline() {
        // Markup headings are presentation, not structure: a pi run putting
        // h-tags in a card row minted nonsense sections. Explicit outlines
        // (v2) are markup's way back in.
        let b = parse_one(
            "<sv-markup id=\"b4\">\n<div class=\"row\"><h5>Card A</h5><h5>Card B</h5></div>\n</sv-markup>",
        );
        assert!(outline("b4", &b).is_empty());
    }

    #[test]
    fn html_block_is_iframed_escaped_and_contributes_no_outline() {
        let b = parse_one(
            "<sv-html id=\"b5\">\n<h1 id=\"inside\">Doc</h1><p class=\"x\">hi &amp; bye</p>\n</sv-html>",
        );
        let headings = outline("b5", &b);
        assert!(headings.is_empty(), "unreachable anchors mint unusable rail entries and tabs");
        // The agent's height attribute becomes the frame's inline style…
        let sized = parse_one(
            "<sv-html id=\"b6\" height=\"40rem\">\n<p>doc</p>\n</sv-html>",
        );
        assert!(block("b6", &sized).contains(r#" style="height:40rem""#));
        // …and a typo'd unit falls back to the css default, never a broken attr.
        let bad = parse_one(
            "<sv-html id=\"b7\" height=\"tall;position:fixed\">\n<p>doc</p>\n</sv-html>",
        );
        assert!(!block("b7", &bad).contains("style=\"height"));
        let html = block("b5", &b);
        assert!(html.contains("sandbox=\"allow-scripts\""));
        assert!(!html.contains(r#"srcdoc="<h1"#), "quotes must be escaped: {html}");
    }
}

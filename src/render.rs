//! The daemon sends rendered HTML, not specs. comrak runs here, the design
//! vocabulary lives here, and the SSE payload is the finished element — so the
//! client never needs a markdown parser and cannot drift out of sync.

use crate::spec::{self, Decoded, Spec};

/// Render a stored block into the element the page inserts. Never errors:
/// an undecodable spec renders as a visibly marked degraded block.
pub fn block(short_id: &str, spec_json: &str) -> String {
    match spec::decode(spec_json) {
        Ok(Decoded::Known(spec)) => known(short_id, &spec),
        Ok(Decoded::Degraded(env)) => degraded(short_id, env.fallback.as_deref()),
        Err(_) => degraded(short_id, None),
    }
}

fn known(short_id: &str, spec: &Spec) -> String {
    let type_name = spec.type_name();
    let body = match spec {
        // Headings get ids prefixed with the block's short id, so outline
        // entries can jump to them and two blocks' "Overview" don't collide.
        Spec::Prose { text } => markdown_opts(text, Some(&format!("{short_id}-"))),
        // Not sanitized; scripts run. A decision, not an oversight — the block
        // is code you own in a page only you can reach. See DESIGN.md.
        Spec::Markup { html } => html.clone(),
        Spec::Html { document } => iframe(document),
    };
    // TODO(v0): pass markup through a lenient HTML parser here and log unknown
    // sv-/Bootstrap classes and inline style= attributes — silent no-ops become
    // a measurement of where the vocabulary falls short. Server-side, because
    // v0 has no browser→server channel and this must not force one.
    format!(
        r#"<section class="sv-block" data-block="{short_id}" data-type="{type_name}">{body}</section>"#
    )
}

fn degraded(short_id: &str, fallback: Option<&str>) -> String {
    let body = match fallback {
        Some(md) => markdown(md),
        None => "<p>(no fallback provided)</p>".to_string(),
    };
    format!(
        r#"<section class="sv-block sv-degraded" data-block="{short_id}" data-type="degraded"><p class="sv-degraded-note">this block needs a newer sideview</p>{body}</section>"#
    )
}

/// GFM via comrak — tables, task lists and strikethrough turn up constantly
/// in plans. `unsafe` because prose is the author's own HTML-bearing
/// markdown, same trust story as markup blocks.
pub fn markdown(text: &str) -> String {
    markdown_opts(text, None)
}

fn markdown_opts(text: &str, header_id_prefix: Option<&str>) -> String {
    let mut options = comrak_options();
    options.extension.header_id_prefix = header_id_prefix.map(str::to_string);
    comrak::markdown_to_html(text, &options)
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
/// backfill. An explicit `headings` field in the envelope is the deferred
/// escape hatch for types that defy autodetection; the envelope already
/// ignores unknown fields, so adding it later is purely additive.
pub fn outline(short_id: &str, spec_json: &str) -> Vec<Heading> {
    match spec::decode(spec_json) {
        Ok(Decoded::Known(Spec::Prose { text })) => {
            prose_outline(&text, &format!("{short_id}-"))
        }
        Ok(Decoded::Known(Spec::Markup { html })) => fragment_outline(&html, true),
        // Real headings, but behind a sandboxed iframe: listed, not anchored.
        Ok(Decoded::Known(Spec::Html { document })) => fragment_outline(&document, false),
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
/// stylesheets injected so isolated blocks look consistent for free — and the
/// same data-bs-theme wiring, since Bootstrap themes entirely off it.
fn iframe(document: &str) -> String {
    let with_style = format!(
        concat!(
            r#"<link rel="stylesheet" href="/assets/vendor/bootstrap.min.css">"#,
            r#"<link rel="stylesheet" href="/assets/sideview.css">"#,
            r#"<script>document.documentElement.setAttribute('data-bs-theme',"#,
            r#"matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light')</script>"#,
            "{}"
        ),
        document
    );
    let srcdoc = attr_escape(&with_style);
    // TODO(v0): size via ResizeObserver + postMessage from a small injected
    // script; the fixed starting height is a placeholder.
    format!(
        r#"<iframe class="sv-html" sandbox="allow-scripts" srcdoc="{srcdoc}" style="width:100%;height:24rem;border:0"></iframe>"#
    )
}

fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_renders_gfm_tables() {
        let html = block("b1", r#"{"type":"prose","text":"| a |\n|---|\n| 1 |","version":1}"#);
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains(r#"data-block="b1""#));
    }

    #[test]
    fn unknown_type_degrades_visibly() {
        let html = block("b2", r#"{"type":"holo","version":9,"fallback":"plain *text*"}"#);
        assert!(html.contains("needs a newer sideview"), "{html}");
        assert!(html.contains("<em>text</em>"), "fallback goes through the prose path: {html}");
    }

    #[test]
    fn prose_outline_ids_match_rendered_anchors() {
        let json = serde_json::json!({
            "type": "prose",
            "text": "## Alpha\n\ntext\n\n### Beta `code`\n\n## Alpha",
            "version": 1,
        })
        .to_string();
        let headings = outline("b3", &json);
        assert_eq!(
            headings.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(),
            vec![(2, "Alpha"), (3, "Beta code"), (2, "Alpha")]
        );
        // Every extracted id must exist in the rendered HTML, dedup included.
        let html = block("b3", &json);
        for h in &headings {
            let anchor = format!(r#"id="{}""#, h.id.as_deref().unwrap());
            assert!(html.contains(&anchor), "missing {anchor} in {html}");
        }
        assert_ne!(headings[0].id, headings[2].id, "duplicate titles must dedup");
    }

    #[test]
    fn markup_outline_honours_author_ids_and_survives_bad_html() {
        let json = r#"{"type":"markup","html":"<h2 id=\"pick-me\">Chosen <b>title</b></h2><div><h3>Loose</h3>","version":1}"#;
        let headings = outline("b4", json);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].id.as_deref(), Some("pick-me"));
        assert_eq!(headings[0].text, "Chosen title");
        assert_eq!(headings[1].id, None);
    }

    #[test]
    fn iframe_html_outline_is_listed_but_never_anchored() {
        let json = r#"{"type":"html","document":"<h1 id=\"inside\">Doc</h1>","version":1}"#;
        let headings = outline("b5", json);
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].id, None, "iframe anchors are unreachable from the page");
    }

    #[test]
    fn html_block_is_iframed_and_escaped() {
        let html = block(
            "b3",
            r#"{"type":"html","document":"<p class=\"x\">hi &amp; bye</p>","version":1}"#,
        );
        assert!(html.contains("sandbox=\"allow-scripts\""));
        assert!(!html.contains(r#"srcdoc="<p class="x""#), "quotes must be escaped: {html}");
    }
}

//! The sv-ask block: agent-led questions answered *at the block* (V4.sv,
//! shape settled by grill 2026-08-19). One tag; `role="close"` renders the
//! round's finish control. Everything here is presentation: answers are
//! drafts in the reader's browser until the finish press, which submits ONE
//! comment for the whole round through the ordinary comment machinery — so
//! `sideview watch` covers asks unchanged and nothing in this module
//! touches the store. Core, zero migration.

use crate::format::Block;
use crate::render::{markdown_opts, text_escape};

/// The close vocabulary, in display order. Started as plannotator's three
/// (approved / approved with notes / revise); the middle one was cut in the
/// round-5 drill (thread 71) — with the note field always present,
/// "approved with notes" is derivable from approved-plus-note, and a
/// derivable verdict is noise. The chosen verdict leads the round comment's
/// first line, so this list is part of the round contract.
pub const VERDICTS: &[&str] = &["approved", "revise"];

/// Render an ask block. Never errors: a malformed round or role degrades
/// visibly with the body kept readable, same law as every other block.
pub fn block(id: &str, b: &Block) -> String {
    let warnings: String = b
        .warnings
        .iter()
        .map(|w| format!(r#"<div class="sv-parse-warning">{}</div>"#, text_escape(w)))
        .collect();

    // Missing round means round 1 (the common one-round page shouldn't have
    // to say so); a malformed one degrades rather than silently re-homing
    // the question in a round the author didn't name.
    let round = match b.attr("round") {
        None => Ok(1u64),
        Some(r) => r.trim().parse::<u64>().map_err(|_| r.to_string()),
    };
    let round = match round {
        Ok(n) if n >= 1 => n,
        _ => {
            let note = degraded(&format!(
                "round=\"{}\" is not a positive integer",
                b.attr("round").unwrap_or("")
            ));
            return format!(
                r#"<section class="sv-block sv-ask sv-degraded" data-block="{id}" data-type="sv-ask">{warnings}{note}{}</section>"#,
                preformatted(&b.body)
            );
        }
    };

    match b.attr("role") {
        None => question(id, b, round, &warnings),
        Some("close") => close(id, b, round, &warnings),
        Some(other) => {
            let note = degraded(&format!(
                "role=\"{}\" — the only role is \"close\"",
                text_escape(other)
            ));
            format!(
                r#"<section class="sv-block sv-ask sv-degraded" data-block="{id}" data-type="sv-ask">{warnings}{note}{}</section>"#,
                preformatted(&b.body)
            )
        }
    }
}

/// A question: body markdown, with every column-0 `- ` line lifted out as an
/// enumerated option, plus the free-text rider every question carries. No
/// options is fine — the rider is the whole answer. `pick="many"` makes the
/// options checkboxes (round-1 drill, thread 67); a `- * ` line is the
/// agent's suggested answer: badged AND pre-selected, so an untouched
/// question submits exactly what the reader sees selected — never an
/// invisible default (the round-1 closing note's ask).
fn question(id: &str, b: &Block, round: u64, warnings: &str) -> String {
    let many = b.attr("pick") == Some("many");
    let mut prose = Vec::new();
    let mut options: Vec<(String, bool)> = Vec::new();
    for line in b.body.lines() {
        match line.strip_prefix("- ") {
            Some(rest) if !rest.trim().is_empty() => {
                let rest = rest.trim();
                match rest.strip_prefix("* ") {
                    Some(text) if !text.trim().is_empty() => {
                        options.push((text.trim().to_string(), true))
                    }
                    _ => options.push((rest.to_string(), false)),
                }
            }
            _ => prose.push(line),
        }
    }
    let q = markdown_opts(&prose.join("\n"), None);
    let ty = if many { "checkbox" } else { "radio" };
    let opts = if options.is_empty() {
        String::new()
    } else {
        let items: String = options
            .iter()
            .enumerate()
            .map(|(i, (o, suggested))| {
                let checked = if *suggested { " checked" } else { "" };
                let badge = if *suggested {
                    r#"<span class="sv-ask-rec">suggested</span>"#
                } else {
                    ""
                };
                format!(
                    r#"<label class="sv-ask-opt"><input type="{ty}" name="sv-ask-{id}" value="{i}"{checked}><span>{}</span>{badge}</label>"#,
                    text_escape(o)
                )
            })
            .collect();
        format!(r#"<div class="sv-ask-options">{items}</div>"#)
    };
    format!(
        r#"<section class="sv-block sv-ask" data-block="{id}" data-type="sv-ask" data-sv-round="{round}">{warnings}<div class="sv-ask-q">{q}</div>{opts}<textarea class="sv-ask-rider" rows="2" placeholder="free text — rides along with your answer"></textarea></section>"#
    )
}

/// The round's finish control: the close vocabulary, an overall note, and
/// the send button. Send stays disabled until a verdict is picked —
/// completion is per-round, and the verdict is what completes it;
/// unanswered questions ride along honestly as `(unanswered)`. The preview
/// retired in round 4 (thread 70): once round comments became machine-mail
/// with suggestions pre-selected on the page, it had no audience.
fn close(id: &str, b: &Block, round: u64, warnings: &str) -> String {
    let preamble = if b.body.trim().is_empty() {
        String::new()
    } else {
        format!(r#"<div class="sv-ask-q">{}</div>"#, markdown_opts(&b.body, None))
    };
    let verdicts: String = VERDICTS
        .iter()
        .map(|v| {
            format!(
                r#"<label class="sv-ask-opt sv-ask-verdict"><input type="radio" name="sv-ask-{id}" value="{v}"><span>{v}</span></label>"#
            )
        })
        .collect();
    format!(
        concat!(
            r#"<section class="sv-block sv-ask sv-ask-close" data-block="{id}" data-type="sv-ask" data-sv-round="{round}" data-sv-role="close">"#,
            "{warnings}{preamble}",
            r#"<div class="sv-ask-verdicts">{verdicts}</div>"#,
            r#"<textarea class="sv-ask-rider" rows="2" placeholder="closing note (optional)"></textarea>"#,
            r#"<div class="sv-ask-actions">"#,
            r#"<button type="button" class="sv-ask-send btn btn-sm btn-primary" disabled>send round {round}</button>"#,
            "</div>",
            r#"<p class="sv-ask-sent" hidden></p>"#,
            "</section>"
        ),
        id = id,
        round = round,
        warnings = warnings,
        preamble = preamble,
        verdicts = verdicts,
    )
}

fn degraded(msg: &str) -> String {
    format!(r#"<p class="sv-degraded-note">{msg}</p>"#)
}

fn preformatted(body: &str) -> String {
    format!("<pre><code>{}</code></pre>", text_escape(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    fn ask(src: &str) -> String {
        let page = format::parse(src);
        block("a1", &page.blocks[0])
    }

    #[test]
    fn question_lifts_options_and_renders_prose_as_markdown() {
        let html = ask(
            "<sv-ask id=\"a1\" round=\"2\">\nWhich **retry** policy?\n- exponential backoff\n- fixed 5s\n- none\n</sv-ask>",
        );
        assert!(html.contains(r#"data-sv-round="2""#));
        assert!(html.contains("<strong>retry</strong>"), "question body is markdown: {html}");
        assert_eq!(html.matches(r#"<label class="sv-ask-opt""#).count(), 3);
        assert!(html.contains(r#"value="0""#) && html.contains("<span>exponential backoff</span>"));
        assert!(html.contains("sv-ask-rider"), "every question carries the free-text rider");
        assert!(!html.contains("<li>"), "option lines never render as a list");
    }

    #[test]
    fn question_without_options_is_free_text_only_and_round_defaults_to_one() {
        let html = ask("<sv-ask id=\"a1\">\nAnything unexpected?\n</sv-ask>");
        assert!(html.contains(r#"data-sv-round="1""#), "missing round means round 1");
        assert!(!html.contains("sv-ask-options"));
        assert!(html.contains("sv-ask-rider"), "the rider is the whole answer");
    }

    #[test]
    fn pick_many_renders_checkboxes_and_suggested_options_preselect() {
        let html = ask(
            "<sv-ask id=\"a1\" pick=\"many\">\nWhich to keep?\n- * the bar\n- the dot\n</sv-ask>",
        );
        assert_eq!(html.matches(r#"type="checkbox""#).count(), 2);
        assert!(!html.contains(r#"type="radio""#));
        // The suggestion is badged AND pre-selected: what an untouched
        // question submits is exactly what the reader sees selected.
        assert!(html.contains(r#"value="0" checked"#), "suggested option starts checked: {html}");
        assert!(html.contains(r#"<span>the bar</span><span class="sv-ask-rec">suggested</span>"#));
        assert!(!html.contains(r#"value="1" checked"#));
        assert!(!html.contains("* the bar"), "the marker never leaks into the label");
    }

    #[test]
    fn options_escape() {
        let html = ask("<sv-ask id=\"a1\">\nQ?\n- <script>x</script>\n</sv-ask>");
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn close_carries_the_vocabulary_and_a_disabled_send() {
        let html = ask("<sv-ask id=\"a1\" round=\"3\" role=\"close\">\nDone? Press send.\n</sv-ask>");
        for v in VERDICTS {
            assert!(html.contains(&format!(r#"value="{v}""#)), "missing verdict {v}: {html}");
        }
        assert!(html.contains("sv-ask-close") && html.contains(r#"data-sv-role="close""#));
        assert!(!html.contains("sv-ask-preview"), "the preview retired in round 4 (thread 70)");
        assert!(html.contains(r#"class="sv-ask-send btn btn-sm btn-primary" disabled"#));
        assert!(html.contains("send round 3"));
    }

    #[test]
    fn malformed_round_and_role_degrade_with_the_body_kept_readable() {
        let html = ask("<sv-ask id=\"a1\" round=\"soon\">\nQ?\n</sv-ask>");
        assert!(html.contains("sv-degraded-note") && html.contains("positive integer"));
        assert!(html.contains("Q?"), "the body stays visible: {html}");
        let html = ask("<sv-ask id=\"a1\" role=\"open\">\nQ?\n</sv-ask>");
        assert!(html.contains("sv-degraded-note") && html.contains("the only role is"));
    }
}

//! The sv-csv block: the table's default tier (V3.sv, settled 2026-08-16).
//! No SQL, no engine — agents always have a tool that emits CSV, so this is
//! only the *view*: server-rendered like every other block, which is what
//! makes it commentable, snapshot-able and zero-JS. Review-scale by
//! requirement: ~2,000 rows is the cap because past that a human cannot
//! review it anyway; wanting more is sqlnow's door.
//!
//! Diffs arrive pre-computed (the agent compares; sideview colors): a
//! `_sv_row` directive column with add/del/mod tints rows in the diff
//! duotone, and `_sv_*` columns are stripped from display.

use crate::format::Block;

/// The requirement, not a limitation (author, 2026-08-16).
pub const MAX_ROWS: usize = 2000;

/// Frozen columns are capped where the stylesheet's rules end.
pub const MAX_FREEZE: usize = 4;

/// Render a CSV block from its file's content. The daemon owns reading the
/// file (and its confinement); a read failure arrives as `Err(reason)` and
/// renders honestly — a missing file heals on the next poll tick, like a
/// missing page.
pub fn block(id: &str, b: &Block, content: Result<String, String>) -> String {
    let src = b.attr("src").unwrap_or("");
    let body = match content {
        Ok(text) => match table(&text, b) {
            Ok(t) => t,
            Err(e) => degraded(&format!("{src}: not readable as CSV — {e}")),
        },
        Err(e) => degraded(&e),
    };
    let warnings: String = b
        .warnings
        .iter()
        .map(|w| format!(r#"<div class="sv-parse-warning">{}</div>"#, esc(w)))
        .collect();
    format!(
        r#"<section class="sv-block" data-block="{id}" data-type="sv-csv">{warnings}{body}</section>"#
    )
}

fn table(text: &str, b: &Block) -> Result<String, csv::Error> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true) // ragged rows render short, not fatal
        .from_reader(text.as_bytes());

    let headers = reader.headers()?.clone();
    // Directive columns configure the paint and never display.
    let directive: Vec<bool> = headers.iter().map(|h| h.starts_with("_sv_")).collect();
    let row_class_col = headers.iter().position(|h| h == "_sv_row");
    let shown: Vec<&str> =
        headers.iter().zip(&directive).filter(|(_, d)| !**d).map(|(h, _)| h).collect();

    let mut rows_html = String::new();
    let mut shown_rows = 0usize;
    let mut total = 0usize;
    // Numeric columns right-align; decided from the rows actually shown.
    let mut numeric: Vec<bool> = shown.iter().map(|_| true).collect();
    let mut cells_by_row: Vec<(Option<String>, Vec<String>)> = Vec::new();

    for record in reader.records() {
        let record = record?;
        total += 1;
        if shown_rows >= MAX_ROWS {
            continue; // keep counting for the honest remainder line
        }
        shown_rows += 1;
        let class = row_class_col
            .and_then(|i| record.get(i))
            .map(str::trim)
            .filter(|v| matches!(*v, "add" | "del" | "mod"))
            .map(str::to_string);
        let mut cells = Vec::with_capacity(shown.len());
        let mut visible = 0usize;
        for (i, val) in record.iter().enumerate() {
            if *directive.get(i).unwrap_or(&false) {
                continue;
            }
            if visible < numeric.len() && !val.trim().is_empty() {
                numeric[visible] &= val.trim().parse::<f64>().is_ok();
            }
            cells.push(val.to_string());
            visible += 1;
        }
        cells_by_row.push((class, cells));
    }

    for (class, cells) in &cells_by_row {
        let tr_class = match class.as_deref() {
            Some("add") => r#" class="sv-csv-add""#,
            Some("del") => r#" class="sv-csv-del""#,
            Some("mod") => r#" class="sv-csv-mod""#,
            _ => "",
        };
        rows_html.push_str(&format!("<tr{tr_class}>"));
        for (i, cell) in cells.iter().enumerate() {
            let num = if *numeric.get(i).unwrap_or(&false) { r#" class="sv-num""# } else { "" };
            rows_html.push_str(&format!("<td{num}>{}</td>", esc(cell)));
        }
        // Ragged short rows: pad so frozen-column offsets stay aligned.
        for _ in cells.len()..shown.len() {
            rows_html.push_str("<td></td>");
        }
        rows_html.push_str("</tr>");
    }

    let head: String = shown
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let num = if *numeric.get(i).unwrap_or(&false) { r#" class="sv-num""# } else { "" };
            format!("<th{num}>{}</th>", esc(h))
        })
        .collect();

    let freeze = b
        .attr("freeze")
        .and_then(|f| f.parse::<usize>().ok())
        .filter(|n| (1..=MAX_FREEZE).contains(n))
        .map(|n| format!(r#" data-sv-freeze="{n}""#))
        .unwrap_or_default();
    let style = b
        .attr("height")
        .filter(|h| crate::render::is_css_length(h))
        .map(|h| format!(r#" style="max-height:{h}" data-sv-scroll="1""#))
        .unwrap_or_default();

    let caption = {
        let src = esc(b.attr("src").unwrap_or(""));
        if total > shown_rows {
            format!(
                r#"<figcaption>showing {shown_rows} of {total} rows — review-scale by design; query a subset for the rest · {src}</figcaption>"#
            )
        } else {
            format!(r#"<figcaption>{total} rows · {src}</figcaption>"#)
        }
    };

    Ok(format!(
        r#"<figure class="sv-csv"{freeze}><div class="sv-csv-scroll"{style}><table><thead><tr>{head}</tr></thead><tbody>{rows_html}</tbody></table></div>{caption}</figure>"#
    ))
}

fn degraded(msg: &str) -> String {
    format!(r#"<p class="sv-degraded-note">{}</p>"#, esc(msg))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    fn csv_block(attrs: &str, body_file_content: &str) -> String {
        let src = format!("<sv-csv id=\"b1\" src=\"data.csv\"{attrs}>\n</sv-csv>");
        let page = format::parse(&src);
        block("b1", &page.blocks[0], Ok(body_file_content.to_string()))
    }

    #[test]
    fn renders_diff_tints_strips_directives_and_aligns_numbers() {
        let html = csv_block(
            "",
            "_sv_row,station,temp\nadd,Berwick,11.2\ndel,Hexham,9.4\nmod,Alnwick,14.9\n,Kelso,8.0\n",
        );
        assert!(html.contains(r#"<tr class="sv-csv-add">"#));
        assert!(html.contains(r#"<tr class="sv-csv-del">"#));
        assert!(html.contains(r#"<tr class="sv-csv-mod">"#));
        assert!(!html.contains("_sv_row"), "directive columns never display");
        assert!(html.contains("<th>station</th>"));
        assert!(html.contains(r#"<th class="sv-num">temp</th>"#), "numeric column right-aligns");
        assert!(html.contains("4 rows"));
    }

    #[test]
    fn caps_at_review_scale_and_says_so() {
        let mut data = String::from("n\n");
        for i in 0..2500 {
            data.push_str(&format!("{i}\n"));
        }
        let html = csv_block("", &data);
        assert!(html.contains("showing 2000 of 2500 rows"), "the cap is the requirement");
        assert_eq!(html.matches("<tr>").count(), 2001, "2000 body rows + the header row");
    }

    #[test]
    fn freeze_height_and_failure_are_honest() {
        let html = csv_block(r#" freeze="2" height="24rem""#, "a,b\n1,2\n");
        assert!(html.contains(r#"data-sv-freeze="2""#));
        assert!(html.contains("max-height:24rem"));
        // Out-of-range freeze is ignored, not clamped silently to something odd.
        let html = csv_block(r#" freeze="9""#, "a\n1\n");
        assert!(!html.contains("data-sv-freeze"));
        // A read failure renders as the honest note and nothing else.
        let src = "<sv-csv id=\"b1\" src=\"gone.csv\">\n</sv-csv>";
        let page = format::parse(src);
        let html = block("b1", &page.blocks[0], Err("no file gone.csv".into()));
        assert!(html.contains("sv-degraded-note") && html.contains("gone.csv"));
        // Cells escape.
        let html = csv_block("", "a\n<script>x</script>\n");
        assert!(html.contains("&lt;script&gt;"));
    }
}

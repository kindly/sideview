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
    // Directive columns configure the paint and never display. `_sqlnow_*`
    // hides too (round 19): sqlnow reserves that prefix the same way
    // (querier's AGENTS.md), and a file carrying its directives must not
    // render them as data here.
    let directive: Vec<bool> = headers
        .iter()
        .map(|h| h.starts_with("_sv_") || h.starts_with("_sqlnow_"))
        .collect();
    let row_class_col = headers.iter().position(|h| h == "_sv_row");
    let shown: Vec<&str> =
        headers.iter().zip(&directive).filter(|(_, d)| !**d).map(|(h, _)| h).collect();
    // `_sv_mark_<col>` marks one cell the way `_sv_row` marks the row (V4.sv;
    // the author's daily data-diff case is a mod row with the changed cells
    // deeper). Named `mark`, not `cell`, because sqlnow's `_sqlnow_cell_` is
    // a rich-JSON widget — a false friend killed in round 19. The alignment
    // decision from the same round: `_sqlnow_format_<col>` is honored as a
    // mark source too — its added/changed/removed vocabulary maps onto
    // add/mod/del, other style words no-op — so one annotated file renders
    // in both viewers. A directive naming no shown column is ignored, like
    // an out-of-range freeze.
    let cell_dirs: Vec<(usize, usize)> = headers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            h.strip_prefix("_sv_mark_").or_else(|| h.strip_prefix("_sqlnow_format_")).map(|col| (i, col))
        })
        .filter_map(|(i, col)| shown.iter().position(|s| *s == col).map(|vis| (i, vis)))
        .collect();

    let mut rows_html = String::new();
    let mut shown_rows = 0usize;
    let mut total = 0usize;
    // Numeric columns right-align; decided from the rows actually shown.
    let mut numeric: Vec<bool> = shown.iter().map(|_| true).collect();
    let mut cells_by_row: Vec<(Option<String>, Vec<String>, Vec<Option<&str>>)> = Vec::new();

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
        let mut cell_class: Vec<Option<&str>> = vec![None; shown.len()];
        for (dir_i, vis) in &cell_dirs {
            // Both vocabularies accepted in both directives — the sideview
            // diff verbs and sqlnow's named styles that mean the same thing.
            cell_class[*vis] = match record.get(*dir_i).map(str::trim) {
                Some("add" | "added") => Some("sv-csv-add"),
                Some("del" | "removed") => Some("sv-csv-del"),
                Some("mod" | "changed") => Some("sv-csv-mod"),
                _ => cell_class[*vis],
            };
        }
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
        cells_by_row.push((class, cells, cell_class));
    }

    for (class, cells, cell_class) in &cells_by_row {
        let tr_class = match class.as_deref() {
            Some("add") => r#" class="sv-csv-add""#,
            Some("del") => r#" class="sv-csv-del""#,
            Some("mod") => r#" class="sv-csv-mod""#,
            _ => "",
        };
        rows_html.push_str(&format!("<tr{tr_class}>"));
        for (i, cell) in cells.iter().enumerate() {
            let mut classes = Vec::new();
            if *numeric.get(i).unwrap_or(&false) {
                classes.push("sv-num");
            }
            if let Some(Some(c)) = cell_class.get(i) {
                classes.push(c);
            }
            let attr = if classes.is_empty() {
                String::new()
            } else {
                format!(r#" class="{}""#, classes.join(" "))
            };
            rows_html.push_str(&format!("<td{attr}>{}</td>", esc(cell)));
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
    fn cell_marks_tint_one_cell_and_never_display() {
        // The daily data-diff: a mod row whose changed cell is marked deeper.
        let html = csv_block(
            "",
            "_sv_row,_sv_mark_temp,station,temp\nmod,mod,Berwick,11.2\n,,Hexham,9.4\n",
        );
        assert!(html.contains(r#"<tr class="sv-csv-mod">"#), "row mark still applies: {html}");
        assert!(
            html.contains(r#"<td class="sv-num sv-csv-mod">11.2</td>"#),
            "the named cell wears the mark: {html}"
        );
        assert!(!html.contains("_sv_mark"), "directive columns never display");
        assert!(!html.contains(r#"sv-csv-mod">9.4"#), "unmarked rows' cells stay bare: {html}");
        // A directive naming no shown column is ignored, like a bad freeze.
        let html = csv_block("", "_sv_mark_ghost,a\nadd,1\n");
        assert!(!html.contains("sv-csv-add"), "unknown column name is a no-op: {html}");
    }

    #[test]
    fn sqlnow_directives_hide_and_format_marks_render() {
        // One annotated file, both viewers (round 19): sqlnow's format
        // directive with its own vocabulary renders as tints here, and every
        // _sqlnow_* column hides — including cell_, the rich-JSON widget
        // sideview doesn't render.
        let html = csv_block(
            "",
            "_sqlnow_format_temp,_sqlnow_cell_temp,station,temp\nchanged,\"{\"\"kind\"\":\"\"bar\"\"}\",Berwick,11.2\nadded,,Kelso,8.0\n",
        );
        assert!(
            html.contains(r#"<td class="sv-num sv-csv-mod">11.2</td>"#),
            "changed → mod tint: {html}"
        );
        assert!(html.contains(r#"sv-csv-add">8.0"#), "added → add tint: {html}");
        assert!(!html.contains("_sqlnow_"), "the whole prefix hides: {html}");
        assert!(!html.contains("bar"), "widget JSON never renders as data: {html}");
        // Style words that aren't marks (heat ramps, warn) no-op quietly.
        let html = csv_block("", "_sqlnow_format_a,a\nheat:0.7,1\n");
        assert!(!html.contains("sv-csv-add") && !html.contains("sv-csv-mod"), "{html}");
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

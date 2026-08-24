//! The block stack (V5.sv): parse `.sv` pages and render blocks — one
//! narrow interface over format, render, and the block types (ask, csv,
//! diff). An internal crate so a change inside it needs none of the
//! daemon's, the CLI's, or the store's context; a pipeline, so it wears no
//! models/logic layering. It knows nothing about SQLite, HTTP, or the
//! filesystem beyond what a caller hands it.

pub mod ask;
pub mod csv;
pub mod diff;
pub mod format;
pub mod render;

/// Percent-encode a page or block id for a URL path segment. Lives here
/// because rendered ext frames embed their own URLs; the binary reuses it
/// for every printed link.
pub fn encode_id(id: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SEGMENT: &AsciiSet =
        &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
    utf8_percent_encode(id, SEGMENT).to_string()
}

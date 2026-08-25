//! The block stack (V5.sv): parse `.sv` pages and render blocks — one
//! narrow interface over format, render, and the block types (ask, csv,
//! diff). A module directory, not a crate (round 4: a crate useful only to
//! this binary would just pollute crates.io; re-cut it as one if the
//! project grows real outside users). A pipeline, so it wears no
//! models/logic layering — and by convention, compiler-unenforced, it
//! knows nothing about SQLite, HTTP, or the daemon: nothing in blocks/
//! may import models, logic, or the interfaces.

pub mod ask;
pub mod csv;
pub mod diff;
pub mod format;
pub mod render;

/// Percent-encode a page or block id for a URL path segment. Lives here
/// because rendered ext frames embed their own URLs; identity::encode
/// delegates here for every printed link.
pub fn encode_id(id: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SEGMENT: &AsciiSet =
        &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
    utf8_percent_encode(id, SEGMENT).to_string()
}

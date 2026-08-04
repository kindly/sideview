//! Block specs and the envelope they sit in.
//!
//! This file is where reference-never-embed is kept or lost. The test for every
//! later variant is whether adding it would open the door: `table` gets `{sql}`,
//! `diff` gets two refs or a path, an `image` that returns gets `{path}` and no
//! `data` or `bytes` field to reach for. A variant that can hold rows is a
//! variant designed wrong, and this is where you would notice.
//!
//! All three v0 types carry content, which is the legitimate exception the rule
//! always had: prose, markup and html hold text the model authored.

use serde::{Deserialize, Serialize};

/// The highest spec version this binary understands. A block whose envelope
/// carries a higher one is rendered via its fallback, never errored.
pub const SPEC_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Spec {
    /// Markdown, rendered server-side by comrak (GFM).
    Prose { text: String },
    /// An HTML fragment, inlined into the page. Inherits the page's styles —
    /// no shadow root, per V0.md.
    Markup { html: String },
    /// A whole document, isolated in a sandboxed iframe.
    Html { document: String },
}

impl Spec {
    /// The single producer of both spellings of the type: the `type` column
    /// and the serde tag must never be written independently of each other.
    pub fn type_name(&self) -> &'static str {
        match self {
            Spec::Prose { .. } => "prose",
            Spec::Markup { .. } => "markup",
            Spec::Html { .. } => "html",
        }
    }
}

/// What every block carries regardless of type. Parsed *first*, so that a
/// renderer meeting an unknown `type` or a too-new `version` already holds
/// `fallback` and can degrade instead of erroring.
///
/// No `deny_unknown_fields`, deliberately: a newer sideview adding a field
/// must not make an older one refuse the block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(flatten)]
    pub rest: serde_json::Value,
}

/// The result of decoding a stored spec: a variant we know at a version we
/// render, or the material to degrade gracefully.
#[derive(Debug)]
pub enum Decoded {
    Known(Spec),
    /// Unknown type, or a version above [`SPEC_VERSION`]. Render the
    /// envelope's `fallback` through the prose path inside a visibly marked
    /// container — "this block needs a newer sideview" — never an empty div
    /// or a thrown error.
    Degraded(Envelope),
}

/// Serialize a spec into the stored `spec_json`: the variant's tagged fields
/// plus the envelope's `version` (and `fallback` when present), as one flat
/// object.
pub fn encode(spec: &Spec, fallback: Option<&str>) -> anyhow::Result<String> {
    let mut value = serde_json::to_value(spec)?;
    let obj = value
        .as_object_mut()
        .expect("a tagged enum serializes to an object");
    obj.insert("version".into(), SPEC_VERSION.into());
    if let Some(fb) = fallback {
        obj.insert("fallback".into(), fb.into());
    }
    Ok(value.to_string())
}

/// Parse the envelope first, then the variant. Failure to parse the variant
/// is *degrade*, not error; only an unparseable envelope is an error.
pub fn decode(spec_json: &str) -> anyhow::Result<Decoded> {
    let envelope: Envelope = serde_json::from_str(spec_json)?;
    if envelope.version > SPEC_VERSION {
        return Ok(Decoded::Degraded(envelope));
    }
    match serde_json::from_str::<Spec>(spec_json) {
        Ok(spec) => Ok(Decoded::Known(spec)),
        Err(_) => Ok(Decoded::Degraded(envelope)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let spec = Spec::Prose { text: "hello".into() };
        let json = encode(&spec, None).unwrap();
        match decode(&json).unwrap() {
            Decoded::Known(s) => assert_eq!(s, spec),
            other => panic!("expected Known, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_degrades_with_fallback_in_hand() {
        let json = r#"{"type":"table","sql":"select 1","version":1,"fallback":"| a |"}"#;
        match decode(json).unwrap() {
            Decoded::Degraded(env) => assert_eq!(env.fallback.as_deref(), Some("| a |")),
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[test]
    fn newer_version_degrades_even_for_a_known_type() {
        let json = format!(
            r#"{{"type":"prose","text":"x","version":{},"fallback":"x"}}"#,
            SPEC_VERSION + 1
        );
        assert!(matches!(decode(&json).unwrap(), Decoded::Degraded(_)));
    }

    #[test]
    fn unknown_fields_are_ignored_not_refused() {
        let json = r#"{"type":"prose","text":"x","version":1,"future_field":true}"#;
        assert!(matches!(decode(json).unwrap(), Decoded::Known(_)));
    }

    #[test]
    fn type_column_comes_from_the_variant() {
        let spec = Spec::Markup { html: "<b>hi</b>".into() };
        let json = encode(&spec, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], spec.type_name());
    }
}

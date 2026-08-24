//! The logic layer (V5.sv, threads 117/125): one file per concept, plus
//! `base` for the uncategorized domains. Everything an interface does passes
//! through here — interfaces (cli, daemon, monitor) import `logic` and
//! nothing below it, checkable from the use lines. Layer-first rather than
//! concept-first deliberately (thread 126): cross-cutting logic that owns no
//! model gets a plain file here, no contortion.

pub mod base;
pub mod conversation;

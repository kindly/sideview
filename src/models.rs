//! The models layer (V5.sv, thread 117/125): one file per concept, each
//! holding its models fused with their algorithms and their SQL. `base`
//! holds the store infrastructure and the domains with no concept of their
//! own. Only the logic layer calls in here.

pub mod base;
pub mod conversation;

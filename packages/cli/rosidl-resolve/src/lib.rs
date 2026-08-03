//! Stage 1 of the codegen pipeline (RFC-0068): **Resolve**.
//!
//! Takes a parsed ROS interface ([`rosidl_parser::ast`]) and the ambient
//! dependency resolver, and produces a language- and target-neutral IR that
//! carries the fully-qualified type name, the canonical type-description
//! closure, and the RIHS01 type hash — the derived facts every backend needs
//! but none should recompute. Target-specific layout is a later stage (Lower).
//!
//! The [`rihs`] module is the REP-2011 RIHS01 engine, relocated here from
//! `rosidl-codegen` (phase-335 W1.a) so hashing lives at the stage that owns
//! it. `rosidl-codegen` re-exports it, so existing `rosidl_codegen::rihs::…`
//! paths are unchanged.

mod resolved;
pub mod rihs;

pub use resolved::{ResolvedAction, ResolvedMessage, ResolvedService};

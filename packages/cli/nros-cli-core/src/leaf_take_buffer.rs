//! Issue 1233 — the runtime take buffer for a CARGO LEAF, over its CLOSURE.
//!
//! # What was missing
//!
//! `NROS_SUBSCRIPTION_BUFFER_SIZE` sizes `RX_BUF`, which is a const generic on
//! every entity and is also `DEFAULT_TX_BUF`. It is derived in
//! `cmake/NanoRosMessageBounds.cmake` and travelled the RESOLVER road only, so
//! a cargo leaf took the 1024-byte crate default for a number its own
//! `generated/` tree can answer — and, because `nros-node`'s arena model reads
//! the same knob, so did its arena.
//!
//! # Why it is not part of the payload-class join
//!
//! [`crate::leaf_payload_classes`] derives over what the leaf SUBSCRIBES to.
//! This one may not: the CMake derivation states the rule at the site
//! (`BASIS closure, always. Narrowing this one is the under-derivation.`) and
//! the reason is structural — one global size serves every entity, the C/C++
//! path is type-erased, and `DEFAULT_TX_BUF` aliases it, so a type this image
//! only PUBLISHES still has to fit. Deriving it from the subscribed set would
//! produce a buffer that is smaller than the image's largest message and fails
//! as `BufferTooSmall` at runtime rather than at build time.
//!
//! The leaf's closure is its `generated/` tree: `nros sync` writes one package
//! per message dependency, each carrying the bound inventory codegen emitted.
//!
//! # Refusing
//!
//! Every failure keeps the crate default, which is the same direction the
//! CMake lane refuses in: the default is a SIZE that may be too small, but it
//! is the size the leaf already had, and inventing a smaller one from a
//! partial table is how a working image starts dropping messages.

use std::path::Path;

use rosidl_codegen::bounds::BoundState;

/// The take buffer for one leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeBuffer {
    /// The largest `rx` bound in the closure, in bytes.
    Derived(usize),
    /// Not sized here. The reason is reported once, at sync.
    Refused { reason: String },
}

impl TakeBuffer {
    pub fn tag(&self) -> &'static str {
        match self {
            TakeBuffer::Derived(_) => "derived",
            TakeBuffer::Refused { .. } => "refused",
        }
    }
}

/// The closure derivation for a leaf on disk.
pub fn take_buffer_for_leaf(leaf: &Path) -> TakeBuffer {
    derive(|| crate::leaf_payload_classes::leaf_bound_inventory(leaf))
}

/// The derivation proper, with its input supplied — so a test can state a bound
/// table without a directory.
pub fn derive(bounds: impl FnOnce() -> Result<Vec<(String, BoundState)>, String>) -> TakeBuffer {
    let table = match bounds() {
        Ok(t) => t,
        Err(reason) => return TakeBuffer::Refused { reason },
    };

    // NO TABLE IS NOT AN ANSWER. `large_count = 0` is a legitimate answer for
    // the payload classes because "this leaf receives nothing" is a fact about
    // the leaf; "this leaf links no priced type" is a fact about what sync
    // could READ, and a leaf with no message dependency still publishes and
    // receives the built-in types through the raw entity API at `RX_BUF`.
    if table.is_empty() {
        return TakeBuffer::Refused {
            reason: "this leaf has no priced types in `generated/`, so there is no closure to \
                     size the take buffer over"
                .to_string(),
        };
    }

    let mut max_rx = 0usize;
    let mut open: Vec<String> = Vec::new();
    for (type_name, bound) in &table {
        match bound {
            BoundState::Bounded { rx, .. } => max_rx = max_rx.max(*rx),
            // One unbounded type in the CLOSURE refuses the whole knob, which
            // is the CMake lane's rule and not a stricter one: the buffer has
            // to hold the largest type the image could touch, so a type whose
            // size is unknown makes the maximum unknown too.
            _ => open.push(format!("{type_name} ({})", bound.tag())),
        }
    }
    if !open.is_empty() {
        return TakeBuffer::Refused {
            reason: format!(
                "{} type(s) in this leaf's closure carry no derived bound, so the largest \
                 type it could send or receive is unknown:\n    {}\n  Bound the member in \
                 its `.msg` (`string<=64`) or cap it `inline` in the package's \
                 `nros-codegen.toml` (RFC-0033).",
                open.len(),
                open.join("\n    ")
            ),
        };
    }
    TakeBuffer::Derived(max_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded(rx: usize) -> BoundState {
        BoundState::Bounded { rx, tx: rx }
    }

    #[test]
    fn largest_rx_in_the_closure_wins_not_the_subscribed_one() {
        // The 8192 type is the one a leaf might only PUBLISH. A subscribed
        // basis would answer 512 and under-size `DEFAULT_TX_BUF`.
        let t = derive(|| {
            Ok(vec![
                ("a/msg/Small".into(), bounded(512)),
                ("a/msg/Big".into(), bounded(8192)),
            ])
        });
        assert_eq!(t, TakeBuffer::Derived(8192));
    }

    #[test]
    fn one_unbounded_type_refuses_the_whole_knob() {
        let t = derive(|| {
            Ok(vec![
                ("a/msg/Small".into(), bounded(512)),
                (
                    "a/msg/Open".into(),
                    BoundState::Unbounded {
                        reason: "member `data` is open".into(),
                    },
                ),
            ])
        });
        let TakeBuffer::Refused { reason } = t else {
            panic!("an unbounded closure member must refuse");
        };
        assert!(
            reason.contains("a/msg/Open"),
            "reason names the type: {reason}"
        );
    }

    #[test]
    fn an_empty_table_refuses_rather_than_answering_zero() {
        let t = derive(|| Ok(Vec::new()));
        assert_eq!(t.tag(), "refused");
    }

    #[test]
    fn a_read_error_is_the_refusal_reason() {
        let t = derive(|| Err("reading x: boom".into()));
        let TakeBuffer::Refused { reason } = t else {
            panic!("a table that did not read must refuse");
        };
        assert_eq!(reason, "reading x: boom");
    }
}

//! phase-457 W5 (issue 1522) — which registration entry point a DECLARED
//! subscription reaches, as ONE value.
//!
//! # The hole this fills
//!
//! W3 made `[[endpoint]] registration_path`'s third row an OBSERVATION:
//! `SubscriptionRequest::in_place_capable` is stated at each of the executor's
//! eleven subscription entry points, read at
//! [`Executor::open_subscription`][open], and reported from there through
//! [`registration_observer`][obs]. A row nobody observed REFUSES, and every
//! consumer then budgets the receive region — the safe direction.
//!
//! The **Rust producer observes nothing**. `nros::record_node_metadata::<C>`
//! runs a component's `register()` against a recording `NodeContext` and opens
//! no executor at all, so no registration happens in the probe and every Rust
//! endpoint's row refuses. That is issue 1522.
//!
//! # Why a classifier is allowed here, when W3 deleted one
//!
//! W3 deleted an inference: the sizing descriptor composed the in-place row
//! from the entry's LANGUAGE, which is wrong for nine of the eleven entry
//! points. That was a SECOND OPINION — a table in the CLI about code it cannot
//! see.
//!
//! The declarative road is not that shape. Its registrar is ONE function —
//! `nros::node_runtime`'s `EntityKind::Subscription` arm — and it lowers every
//! declared subscription to exactly one of two entry points. So the shape a
//! declared endpoint takes is a FUNCTION OF THE DECLARATION, computed in one
//! place, and this enum is that function's codomain.
//!
//! It stays one opinion only because of the two rules below, both gated by
//! `check-declared-subscription-shape`:
//!
//! 1. **The registrar branches on it.** The subscription arm does not test the
//!    declaration's `safety` flag itself; it asks for a
//!    [`DeclaredSubscriptionShape`] and dispatches on the answer. A variant it
//!    does not name is a gate failure, not a silent fall-through.
//! 2. **The entry point takes its value from here.** Each entry point's
//!    `SubscriptionRequest::in_place_capable` is spelled
//!    `DeclaredSubscriptionShape::<V>.in_place_capable()`, so there is exactly
//!    one `bool` per shape in the tree. Issue 1340's first candidate — letting
//!    the raw buffered path take the in-place row — is then a one-line change
//!    here that moves the entry point and the probe together, which is the
//!    property that makes this cheaper than teaching the probe to register.
//!
//! [open]: super::Executor
//! [obs]: super::registration_observer

/// Which of the executor's registration entry points a DECLARED subscription
/// lowers to.
///
/// One variant per lowering the declarative registrar can choose. Exhaustive:
/// a new lowering is a new variant, which breaks
/// [`in_place_capable`](Self::in_place_capable) at compile time and the
/// registrar at its gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredSubscriptionShape {
    /// `Node::create_generic_subscription_with_qos` →
    /// `Executor::register_subscription_buffered_raw_on`.
    BufferedRaw,
    /// A `.safety()` declaration:
    /// `Node::create_generic_subscription_with_integrity` →
    /// `Executor::register_subscription_buffered_raw_safety_on`.
    BufferedRawSafety,
}

impl DeclaredSubscriptionShape {
    /// Classify a declaration.
    ///
    /// `safety` is the declaration's `.safety()` flag **as the runtime will
    /// see it**: a build whose `safety-e2e` capability is off ignores the flag
    /// and registers the basic path, so its caller masks the flag to `false`
    /// before asking. That mask has exactly one home on the caller's side, for
    /// the same reason this classifier has one here.
    pub const fn of_declaration(safety: bool) -> Self {
        if safety {
            Self::BufferedRawSafety
        } else {
            Self::BufferedRaw
        }
    }

    /// Can a registration of this shape dispatch out of the backend's own
    /// receive slot?
    ///
    /// **The definition, not a copy of one.** Each entry point writes this
    /// call into its `SubscriptionRequest`, so the answer a probe states and
    /// the answer the executor acts on are the same expression.
    ///
    /// Both shapes answer `false` today, and that is worth a sentence rather
    /// than a collapse into `false`: they are false for DIFFERENT reasons —
    /// the raw path because its arena entry is shared with
    /// `add_arena_subscription_callback`, the safety path because
    /// `process_raw_in_place` has no validating form — and only the first is a
    /// limit anyone plans to lift (issue 1340).
    pub const fn in_place_capable(self) -> bool {
        match self {
            Self::BufferedRaw => false,
            Self::BufferedRawSafety => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_classifies_by_its_safety_flag() {
        assert_eq!(
            DeclaredSubscriptionShape::of_declaration(false),
            DeclaredSubscriptionShape::BufferedRaw
        );
        assert_eq!(
            DeclaredSubscriptionShape::of_declaration(true),
            DeclaredSubscriptionShape::BufferedRawSafety
        );
    }

    /// Not a restatement of `in_place_capable`'s body: this is the claim the
    /// PROBE ships, and issue 1522's whole point is that closing it states
    /// `unbounded` rather than unlocking a saving. When issue 1340 flips the
    /// raw arm, this test is what says out loud that every Rust image's
    /// declared subscriptions just changed price.
    #[test]
    fn neither_declarative_shape_dispatches_in_place_today() {
        assert!(!DeclaredSubscriptionShape::BufferedRaw.in_place_capable());
        assert!(!DeclaredSubscriptionShape::BufferedRawSafety.in_place_capable());
    }
}

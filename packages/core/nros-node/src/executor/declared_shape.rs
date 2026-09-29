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
//!    one `bool` per shape in the tree, and the entry point and the probe move
//!    together when it changes — which is what makes this cheaper than
//!    teaching the probe to register.
//!
//!    **Moving together is necessary, not sufficient.** Issue 1340 flipped
//!    `BufferedRaw` to `true`, and this doc used to call that "a one-line
//!    change here". It was not, and the one-line version is UNSAFE: the probe
//!    would state `in_place`, the build would price the row at no receive
//!    region, and the executor — which then ignored `SubscriptionOpen::in_place`
//!    on this path — would still build a buffered entry and claim the full
//!    region. That is the under-size direction, `BufferTooSmall` at boot. The
//!    flip is safe only because `register_subscription_buffered_raw_on` now
//!    BRANCHES on the report; a `true` here is a promise that the entry point
//!    acts on it. `a_generic_subscription_on_an_in_place_backend_claims_no_
//!    receive_region` fails first on a flip without the branch.
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
    /// The two shapes differ, and the reason each has its answer is worth a
    /// sentence:
    ///
    /// * **`BufferedRaw` — `true`** (issue 1340). Its callback is
    ///   `FnMut(&[u8])`, borrowed bytes and nothing else, which is exactly what
    ///   `process_raw_in_place` hands over. It was `false` while its arena entry
    ///   was believed shared with `add_arena_subscription_callback`; that caller
    ///   never reaches `open_subscription`, so the in-place arm could be local to
    ///   the one entry point. A true here is a claim the build PRICES: the
    ///   probe states `in_place` for every declared non-safety subscription, and
    ///   on an in-place backend that endpoint reserves no receive region.
    /// * **`BufferedRawSafety` — `false`.** `process_raw_in_place` has no
    ///   validating form: an integrity-checked sample must be verified before
    ///   the callback sees it, and the borrowed slot offers nowhere to put the
    ///   verdict. Not a limit anyone plans to lift by flipping this.
    pub const fn in_place_capable(self) -> bool {
        match self {
            Self::BufferedRaw => true,
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
    /// PROBE ships, so it is what every Rust image's declared subscriptions are
    /// PRICED at.
    ///
    /// It was `neither_declarative_shape_dispatches_in_place_today` until issue
    /// 1340 flipped the raw arm, and that rename is the point: this test is
    /// where a change of price is said out loud. A declared non-safety
    /// subscription now states `in_place`, so on zenoh and XRCE it claims no
    /// receive region; a `.safety()` one still buffers.
    #[test]
    fn only_the_non_safety_declarative_shape_dispatches_in_place() {
        assert!(DeclaredSubscriptionShape::BufferedRaw.in_place_capable());
        assert!(!DeclaredSubscriptionShape::BufferedRawSafety.in_place_capable());
    }
}

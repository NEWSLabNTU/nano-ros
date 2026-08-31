//! Phase 392 W3b — `rx_buffer_for!` names a subscription's buffer size from the
//! message type, so the number cannot drift from the schema.
//!
//! An INTEGRATION test rather than a unit test, deliberately: a `macro_rules!`
//! body expands in the caller's crate, so the only way to prove it reaches
//! everything it needs — `__rx_bound`, `DEFAULT_RX_BUF_SIZE`, `Option` — is to
//! expand it from outside `nros`. A unit test inside the crate would resolve
//! names that a real consumer cannot see and report a green that means nothing.

use nros_serdes::schema::{Field, FieldType, Message};

const fn f(name: &'static str, ty: FieldType) -> Field {
    Field {
        name,
        ty,
        offset: 0,
    }
}

/// Every member fixed-width, so the type has a bound.
struct Bounded;
impl Message for Bounded {
    const TYPE_NAME: &'static str = "test/msg/Bounded";
    const FIELDS: &'static [Field] = &[
        f("a", FieldType::Uint64),
        f("b", FieldType::Uint32),
        f("c", FieldType::Uint8),
    ];
}

/// A `String` member: no bound exists at any size.
struct Unbounded;
impl Message for Unbounded {
    const TYPE_NAME: &'static str = "test/msg/Unbounded";
    const FIELDS: &'static [Field] = &[f("s", FieldType::String)];
}

#[test]
fn a_bounded_type_sizes_itself() {
    let expected = nros_serdes::size::max_serialized_bound::<Bounded>()
        .expect("a fixed-width type is bounded");
    assert_eq!(
        nros::rx_buffer_for!(Bounded),
        expected,
        "the macro must expand to the type's own bound, not to the default"
    );
    assert_ne!(
        nros::rx_buffer_for!(Bounded),
        nros::DEFAULT_RX_BUF_SIZE,
        "this fixture is only meaningful while the bound differs from the \
         default — otherwise the assertion above passes for the wrong reason"
    );
}

/// Phase-403 W0 INVERTS this test. It was
/// `an_unbounded_type_keeps_the_configured_default`, and it asserted the exact
/// behaviour that is now refused.
///
/// The old rule read phase 380 as licensing the configured default: `None`
/// means "no bound EXISTS", the macro must not invent a number, so hand back
/// the number the subscription would have used anyway. The project owner has
/// since ruled the other way, and the reasoning is the same rule read to its
/// conclusion: a receive buffer sized from a fallback is a buffer that silently
/// mismatches the wire, so substituting `DEFAULT_RX_BUF_SIZE` was itself the
/// invention phase 380 forbade. Every message type must carry a derived upper
/// bound -- in the `.msg` (`string<=64`) or as a `cap` in `nros-codegen.toml` --
/// and a type with none is a BUILD ERROR.
///
/// This is a RENAME, not a deletion: the safety property the old test guarded
/// (an unbounded type never silently gets a plausible-looking size) is stronger
/// now, not dropped.
///
/// What can be asserted HERE is the value the macro branches on. The refusal
/// itself cannot be: `nros::rx_buffer_for!(Unbounded)` no longer compiles, and
/// this workspace has no compile-fail harness for integration tests. That half
/// is gated by the `compile_fail` doctest on `rx_buffer_for!` in
/// `packages/api/nros/src/lib.rs`, which carries a compiling positive control
/// beside it so it cannot pass for the wrong reason.
#[test]
fn an_unbounded_type_is_refused_rather_than_defaulted() {
    assert_eq!(
        nros_serdes::size::max_serialized_bound::<Unbounded>(),
        None,
        "the fixture must be unbounded, or the macro never reaches the arm \
         that refuses"
    );
}

/// The point of the whole exercise: usable where a buffer is actually declared.
///
/// `.rx_buffer::<N>()` takes a const generic, and inside the builder `M` is a
/// generic parameter, which stable Rust forbids in a const operation. Expanding
/// at a call site where the type is CONCRETE is what makes it legal, and this
/// stands in for that call site without needing a live node.
struct Buf<const N: usize>;
impl<const N: usize> Buf<N> {
    const fn capacity(&self) -> usize {
        N
    }
}

#[test]
fn the_bound_is_usable_as_a_const_generic_argument() {
    let buf = Buf::<{ nros::rx_buffer_for!(Bounded) }>;
    assert_eq!(
        buf.capacity(),
        nros_serdes::size::max_serialized_bound::<Bounded>().unwrap(),
        "if this ever fails to COMPILE, the macro has stopped being usable in \
         the position it exists for"
    );

    // The unbounded case used to be exercised here as `Buf::<{
    // rx_buffer_for!(Unbounded) }>`, asserting it expanded to
    // `DEFAULT_RX_BUF_SIZE`. Under phase-403 W0 that no longer compiles, which
    // is the point; see `an_unbounded_type_is_refused_rather_than_defaulted`.
}

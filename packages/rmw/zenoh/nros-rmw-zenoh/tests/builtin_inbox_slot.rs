//! Issue 1352 — the parameter family's inbox slot on zenoh.
//!
//! The rule is `build_param_slot.rs`, which `build.rs` includes; a build
//! script carries no `#[cfg(test)]` module that anything runs, so this test
//! includes the same file. Pure: no environment, no build.

#[path = "../build_param_slot.rs"]
mod param_slot;

use param_slot::{BuiltinSlot, builtin_slot};

/// The single table's slot before phase-461 (`SERVICE_BUFFER_SIZE_DEFAULT`).
const FLOOR: usize = 1024;

/// One node, 25 integer parameters with 35-byte names: 875 name bytes. The
/// shape token is `params:name_bytes:prefixes:prefix_bytes:` then the five
/// counts of string / array parameters.
const TWENTY_FIVE_INTEGERS: &str = "25:875:0:0:0:0:0:0:0";

#[test]
fn a_stated_slot_wins_over_everything() {
    assert_eq!(
        builtin_slot(Some(4096), Some(TWENTY_FIVE_INTEGERS), 20, FLOOR),
        BuiltinSlot::Stated(4096)
    );
}

/// The issue's own case, on the road that CAN price it: a `set_parameters`
/// naming all 25 is `11 + 875 + 25 x (8 + 53)` = 2,411 B, rounded to 2,412,
/// so it gets a slot it fits instead of the flat 1,024 B that dropped it.
#[test]
fn twenty_five_declared_integers_derive_a_slot_their_set_request_fits() {
    let slot = builtin_slot(None, Some(TWENTY_FIVE_INTEGERS), FLOOR, FLOOR);
    assert_eq!(slot, BuiltinSlot::Derived(2412));
}

/// Issue 1352's regression: phase-461 W3 unfloored the user-service slot to
/// the user services' DEMAND, and the builtin family fell back to it. One
/// declared `AddTwoInts` server (~20 B) then sized every parameter service's
/// slot at ~20 B. The fallback may be raised by the user slot, never lowered.
#[test]
fn an_undeclared_family_never_falls_below_the_floor() {
    assert_eq!(
        builtin_slot(None, None, 20, FLOOR),
        BuiltinSlot::Unpriced(FLOOR)
    );
    assert_eq!(
        builtin_slot(None, Some("  "), 20, FLOOR),
        BuiltinSlot::Unpriced(FLOOR)
    );
    assert_eq!(
        builtin_slot(None, None, 2048, FLOOR),
        BuiltinSlot::Unpriced(2048)
    );
}

/// Measured on main: 25 declared parameters, ONE a string, and this crate
/// abstained to the 1,024 B slot, which dropped the 25-name `set_parameters`
/// with no reply while nros-node had priced it. A declaration this crate
/// cannot price now stops the build and names the knob nros-node checks.
#[test]
fn a_declared_string_parameter_refuses_rather_than_guessing() {
    let BuiltinSlot::Refused(why) = builtin_slot(None, Some("25:875:0:0:1:0:0:0:0"), FLOOR, FLOOR)
    else {
        panic!("a string parameter must refuse, not guess");
    };
    assert!(why.contains("NROS_PARAM_SERVICE_INBOX_BYTES"), "{why}");
    assert!(why.contains("issue 1352"), "{why}");
    // Every array kind abstains the same way.
    for shape in [
        "1:4:0:0:0:1:0:0:0",
        "1:4:0:0:0:0:1:0:0",
        "1:4:0:0:0:0:0:1:0",
        "1:4:0:0:0:0:0:0:1",
    ] {
        assert!(
            matches!(
                builtin_slot(None, Some(shape), FLOOR, FLOOR),
                BuiltinSlot::Refused(_)
            ),
            "{shape}"
        );
    }
    // ...and a statement clears it.
    assert_eq!(
        builtin_slot(Some(4096), Some("25:875:0:0:1:0:0:0:0"), FLOOR, FLOOR),
        BuiltinSlot::Stated(4096)
    );
}

/// With no builtin table (the application's queryable count unknown), the
/// parameter services draw user-service rings, so a priced builtin slot must
/// reach them. Measured before: `BUILTIN_INBOX_BYTES` 4096 stated, no table,
/// and a 2,411-byte `set_parameters` dropped by a 1,024 B user ring.
#[test]
fn a_priced_builtin_slot_reaches_the_user_rings_when_there_is_no_builtin_table() {
    use param_slot::user_slot_without_builtin_table as shared;
    // "Priced" is a statement or a derivation, never the floor.
    assert!(BuiltinSlot::Stated(1).is_priced() && BuiltinSlot::Derived(1).is_priced());
    assert!(!BuiltinSlot::Unpriced(1024).is_priced());
    // No table, priced: the user rings carry the family.
    assert_eq!(shared(false, true, 4096, 1024), 4096);
    // A table exists: the user rings keep their own size.
    assert_eq!(shared(true, true, 4096, 1024), 1024);
    // Unpriced: nothing to carry.
    assert_eq!(shared(false, false, 4096, 1024), 1024);
    // Never lowers.
    assert_eq!(shared(false, true, 512, 1024), 1024);
}

/// A malformed token is nros-node's to refuse (it owns the grammar); here it
/// must not be reported as a string-parameter problem.
#[test]
fn a_malformed_shape_is_not_this_crates_refusal() {
    assert_eq!(
        builtin_slot(None, Some("25:875:0"), FLOOR, FLOOR),
        BuiltinSlot::Unpriced(FLOOR)
    );
}

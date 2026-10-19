//! Issue 1971 -- the arena floor applies only where the demand is unknown.
//!
//! Drives the SAME rule the build script runs (`build/arena_floor.rs`,
//! included here by `#[path]` exactly as `build.rs` includes it): a declared
//! image's arena and a tier's arena are their model, unfloored, and an
//! undeclared image's worst-case budget is floored as before.

#[path = "../build/arena_floor.rs"]
mod arena_floor;

use arena_floor::{ARENA_FLOOR, arena_for_demand, tier_arena};

/// The safety island's `estop` tier (RFC-0107): its model is 7,500 B, and the
/// floor raised it to 8,192.
const ESTOP_MODEL: usize = 7_500;

#[test]
fn a_declared_image_arena_is_its_model_below_the_floor() {
    const { assert!(ESTOP_MODEL < ARENA_FLOOR) };
    assert_eq!(arena_for_demand(Some(ESTOP_MODEL), 74_240), ESTOP_MODEL);
}

#[test]
fn a_declared_image_arena_is_its_model_above_the_floor() {
    assert_eq!(arena_for_demand(Some(25_724), 0), 25_724);
}

#[test]
fn a_tier_arena_is_its_model_unfloored() {
    assert_eq!(tier_arena(Some(ESTOP_MODEL)), Some(ESTOP_MODEL));
    assert_eq!(tier_arena(Some(20_272)), Some(20_272));
}

#[test]
fn a_tier_whose_counts_are_unknown_is_not_floored_into_a_guess() {
    assert_eq!(tier_arena(None), None);
}

#[test]
fn an_undeclared_image_keeps_the_floored_budget() {
    // Below the floor: the floor.
    assert_eq!(arena_for_demand(None, 2_048), ARENA_FLOOR);
    // Above it: the budget, byte for byte.
    assert_eq!(arena_for_demand(None, 74_240), 74_240);
}

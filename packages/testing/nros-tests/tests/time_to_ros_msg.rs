//! phase-467 Q3 — `nros_core::Time::to_ros_msg()` reaches a REAL generated
//! `builtin_interfaces/msg/Time`.
//!
//! # Why this test exists where it does
//!
//! `nros-core` declares the trait and cannot test it against a message: every
//! generated message crate depends on `nros-core`, so the dependency the other
//! way is a cycle, and that cycle is the whole reason the conversion is a trait
//! codegen implements rather than an inherent method. `nros-core`'s own unit
//! tests therefore check the trait against a hand-written stand-in struct
//! (`time::tests::FakeTimeMsg`), which proves the METHOD and proves nothing
//! about the EMISSION.
//!
//! This crate path-deps the committed pre-generated `nros-builtin-interfaces`,
//! so here — and only here — the two halves meet: the trait from the runtime,
//! the impl from `rosidl-codegen`. If the template's `is_sec_nanosec_msg` arm
//! ever stops rendering, this stops COMPILING. That is the intended failure
//! mode; a codegen branch nothing builds is a branch that silently dies
//! (CLAUDE.md, "a gate that WORKS is not a gate that RUNS").
//!
//! # What is deliberately NOT here
//!
//! No `?`, no `.unwrap()`. `rclrs::Time::to_ros_msg` returns
//! `Result<_, TryFromIntError>` because its `Time` holds `i64` nanoseconds and
//! narrowing to `sec: i32` can overflow; ours already IS `{ sec: i32,
//! nanosec: u32 }`. Ledger row `rust:Time::to_ros_msg` records the missing
//! `Result` as the divergence, and a ported `t.to_ros_msg()?` is meant to fail
//! to compile rather than to be quietly accommodated.

use nros::Time;
use nros_builtin_interfaces::msg::{Duration as DurationMsg, Time as TimeMsg};

#[test]
fn a_core_time_becomes_a_generated_builtin_interfaces_time() {
    let t = Time::new(1_712_345_678, 987_654_321);
    // Type annotation only — the call carries no turbofish, because this is the
    // shape a publisher writes: `msg.header.stamp = node.now().to_ros_msg();`.
    let stamp: TimeMsg = t.to_ros_msg();
    assert_eq!(stamp.sec, 1_712_345_678);
    assert_eq!(stamp.nanosec, 987_654_321);
}

#[test]
fn the_conversion_agrees_with_the_field_set_it_claims_to_copy() {
    // Asserted against the SOURCE fields rather than against restated literals:
    // the claim this row makes is that the message IS the core type's field set,
    // so the two must read alike at the edges — both signs of `sec` and the top
    // of the nanosecond range.
    for (sec, nanosec) in [
        (0, 0),
        (-1, 999_999_999),
        (i32::MIN, 0),
        (i32::MAX, 999_999_999),
    ] {
        let t = Time::new(sec, nanosec);
        let stamp: TimeMsg = t.to_ros_msg();
        assert_eq!(
            (stamp.sec, stamp.nanosec),
            (t.sec, t.nanosec),
            "sec={sec} nanosec={nanosec}"
        );
    }
}

#[test]
fn the_duration_twin_gets_the_impl_too_because_the_predicate_is_structural() {
    // `builtin_interfaces/msg/Duration` is the same two lines upstream as
    // `Time.msg`, so the shape-keyed predicate emits the impl for it as well —
    // exactly as the C++ `template <typename TimeMsgT> Time::to_msg(TimeMsgT&)`
    // binds to it. Asserted rather than merely tolerated: if the predicate is
    // ever narrowed to a package/message NAME, this is the test that says so.
    let d: DurationMsg = Time::new(3, 4).to_ros_msg();
    assert_eq!((d.sec, d.nanosec), (3, 4));
}

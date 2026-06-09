//! Cross-language shared-state roundtrip over ONE generated region (228.D.2).
//!
//! Proves the bake-generated `[[shared_state]]` surface — `nros_shared_state.rs`
//! (Rust accessors + `#[unsafe(no_mangle)] extern "C"` exports) and
//! `nros_shared_context.h` (the matching C typedef + decls) — links and shares a
//! single `LockedSharedRegion` across Rust and C: a Rust write is seen by C, a C
//! write is seen by Rust, and a guarded C `modify` is observed by Rust.
//!
//! build.rs copies the generated module to `src/generated.rs`; it is pulled in
//! as a true module so its inner `#![allow(dead_code)]` stays valid. The C-ABI
//! accessors export globally regardless of the module path, so the C side links
//! them by their C names.

#[path = "generated.rs"]
mod shared;
use shared::{VehicleState, vehicle_state_get, vehicle_state_set};

unsafe extern "C" {
    fn c_write_state(speed: f32, heading: f32, ticks: u32);
    fn c_read_speed() -> f32;
    fn c_read_ticks() -> u32;
    fn c_modify_bump();
}

fn main() {
    // Rust writes -> C reads the SAME region.
    vehicle_state_set(&VehicleState {
        speed: 12.5,
        heading: 1.0,
        ticks: 7,
    });
    let (cs, ct) = unsafe { (c_read_speed(), c_read_ticks()) };
    assert!((cs - 12.5).abs() < 1e-6, "C read speed {cs}, want 12.5");
    assert_eq!(ct, 7, "C read ticks {ct}, want 7");

    // C writes -> Rust reads the SAME region.
    unsafe { c_write_state(99.0, 2.0, 42) };
    let v = vehicle_state_get();
    assert!(
        (v.speed - 99.0).abs() < 1e-6,
        "Rust read speed {}, want 99",
        v.speed
    );
    assert_eq!(v.ticks, 42, "Rust read ticks {}, want 42", v.ticks);

    // Guarded C modify -> Rust observes the increment.
    unsafe { c_modify_bump() };
    assert_eq!(
        vehicle_state_get().ticks,
        43,
        "C modify did not bump under the lock"
    );

    println!("xlang shared-state roundtrip OK");
}

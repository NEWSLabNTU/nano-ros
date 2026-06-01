//! Phase 212.H.2 fixture — talker component stub.
//!
//! Marked with the placeholder `#[nros::component]` attribute the
//! Phase 212.C / 212.E pipeline expects. Body is intentionally trivial
//! — the Phase 212.H.2 audit verifies the *build pipeline* shape, not
//! runtime publishing (that is exercised by the existing
//! `examples/qemu-arm-nuttx/rust/talker/` end-to-end fixture).

#![no_std]

#[unsafe(no_mangle)]
pub extern "C" fn nros_component_talker(_ctx: *mut core::ffi::c_void) -> i32 {
    0
}

//! Phase 212.H.1 fixture — talker component stub.
//!
//! `#[nros::component]` is the eventual Phase 212.C attribute that
//! emits the register glue. For this fixture (which tests the *shim*,
//! not the codegen) we hand-stub the entry symbol so the linker is
//! satisfied even when the codegen-system verb is a no-op.

#![no_std]

/// Registration entry symbol referenced by the generated `system_main.c`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_component_talker() -> i32 {
    0
}

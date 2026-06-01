//! Phase 212.H.3 fixture component — talker.
//!
//! Declarative-only marker: the embedded `firmware/` binary doesn't
//! consume this crate (component registration is emitted by the BSP's
//! `build.rs` into `system_main.c`). Lives in the workspace so
//! `nros plan` can discover it.

pub fn name() -> &'static str {
    "talker"
}

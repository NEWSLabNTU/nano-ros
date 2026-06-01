//! Phase 212.D fixture — minimal Rust component stub.
//!
//! Exposes a no_mangle entry that the cmake side could resolve via
//! corrosion_link_libraries. Link-correctness is what the integration
//! test verifies.

#[unsafe(no_mangle)]
pub extern "C" fn nros_component_talker() -> i32 {
    0
}

//! Emit `memory.x` into OUT_DIR so `cortex-m-rt`'s `link.x` finds it without
//! depending on the board crate's build script (phase 88.15.a).
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

fn main() {
    nros_build_paths::link_script!("memory.x");
}

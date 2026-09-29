//! Build script for nros-board-mps2-an385
//!
//! Copies the mps2-an385.x linker script to the output directory as memory.x
//! so that cortex-m-rt can find it during linking.
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

fn main() {
    nros_build_paths::link_script!("mps2-an385.x" => "memory.x");
}

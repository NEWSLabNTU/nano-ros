//! Build script for nros-board-mps2-an385
//!
//! Copies the mps2-an385.x linker script to the output directory as memory.x
//! so that cortex-m-rt can find it during linking.
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

fn main() {
    // phase-484 W4b-2 (RFC-0103 D10) — the network defaults: this crate's
    // `[board.net]` (`network = "qemu"`) over the platform default, written to
    // `$OUT_DIR/nros_board_net.rs` for `src/config.rs`.
    nros_board_common::board_net::emit_for_crate(
        nros_board_common::board_net::ResolvedNet::platform_default(),
    );
    nros_build_paths::link_script!("mps2-an385.x" => "memory.x");
}

//! Build script for `nros-board-esp32-qemu`.
//!
//! phase-484 W4b-2 (RFC-0103 D10) — the board's network defaults, ONCE: its
//! `[board.net]` (`network = "qemu"`, QEMU's own SLIRP network) over the
//! platform default, written to `$OUT_DIR/nros_board_net.rs` for
//! `src/config.rs` and `src/board_entry.rs`. Both used to restate them — and
//! disagreed (`192.0.3.10` against `10.0.2.50`).

fn main() {
    nros_board_common::board_net::emit_for_crate(
        nros_board_common::board_net::ResolvedNet::platform_default(),
    );
}

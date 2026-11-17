//! RFC-0132 W9.c -- the `linux` board's CAN frame probe: a process linking this
//! board's `socketcan` backend sends a classic and an FD frame.
//!
//! Not a unit test: it needs a SocketCAN interface, so it builds only with the
//! `can-probe` feature and is driven by `scripts/test/can-frame-socketcan.sh`,
//! which checks the frames with `candump`.
//!
//! `cargo test -p nros-board-linux --features can-probe --test can_socketcan_probe -- vcan0 [<classic-iface>]`

// The board's rlib carries the `socketcan` archive's link line; naming the
// crate is what puts it in this process.
extern crate nros_board_linux as _;

#[path = "support/can_probe.rs"]
mod can_probe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(why) = can_probe::can_probe(&args) {
        eprintln!("CAN-PROBE: FAIL: {why}");
        std::process::exit(1);
    }
}

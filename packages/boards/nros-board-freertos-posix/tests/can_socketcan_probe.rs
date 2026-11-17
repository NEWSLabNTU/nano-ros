//! RFC-0132 W9.c -- the `freertos-posix` board's CAN frame probe: the board's
//! FreeRTOS kernel boots (`FreertosPosix::run_bare`), and a FreeRTOS task sends
//! a classic and an FD frame through the `socketcan` backend this board
//! compiles from `nros-board-linux/c/can_socketcan.c`.
//!
//! Not a unit test: it needs a SocketCAN interface and the board's kernel
//! build (`FREERTOS_PORT=GCC_POSIX`, `FREERTOS_CONFIG_DIR`), so it builds only
//! with the `can-probe` feature and is driven by
//! `scripts/test/can-frame-socketcan.sh`, which checks the frames with
//! `candump`. The probe body is the linux board's, so both images send the
//! same frames.

use nros_board_freertos_posix::{Config, FreertosPosix};

#[path = "../../nros-board-linux/tests/support/can_probe.rs"]
mod can_probe;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Exits the process: 0 when the probe passes, 1 with the reason otherwise.
    let _ = FreertosPosix::run_bare(Config::default(), move |_| can_probe::can_probe(&args));
}

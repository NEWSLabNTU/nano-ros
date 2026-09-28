//! Build script for nros-board-mps3-an536-freertos (phase-385 W1).
//!
//! Per-board wiring only. phase-471 W2 — the recipe itself is
//! `nros_board_common::freertos_build::run_overlay`, the runner the FreeRTOS
//! family gained to match the one NuttX and ThreadX-RISCV have had since
//! phase-337; this file states only what is true of THIS board. Everything
//! generic (cflag resolution via the `[arch.cortex-r52]` profile, FreeRTOS/lwIP
//! include dirs, the `NROS_APP_CONFIG` emitter, newlib discovery for the right
//! multilib) lives there; the FreeRTOS kernel, lwIP, `nros-platform-freertos`
//! and the generic C glue are compiled by `nros-board-freertos/build.rs` and
//! propagate transitively.
//!
//! Kernel + port: the in-tree `GCC/ARM_CRx_No_GIC`, with no override
//! expected. Unlike the S32Z270 sibling there is no licensing seam here —
//! that port needs a board-supplied tick and GIC, which is exactly what
//! `c/board_an536.c` provides, so a clean checkout is RUNNABLE, not merely
//! link-complete. `FREERTOS_DIR`/`FREERTOS_PORT` still honour the family
//! convention for anyone experimenting with another port.

use nros_board_common::freertos_build::{Overlay, OverlayEnv, run_overlay};

fn main() {
    run_overlay(&Overlay {
        // issue 1561 — `board_an536.c` includes `lan9118_lwip.h` for its strong
        // netif overrides. This overlay was copied from the S32Z270 one, whose
        // netif is consumer-side and which therefore adds no such include, so
        // the crate could not compile its own board C file from cargo at all.
        // `cmake/board/nano-ros-board-mps3-an536-freertos.cmake` has carried
        // the same include, with the same reason, since phase-385; the fact now
        // holds on both roads. The driver `.c` stays CMake's (`lan9118_lwip`).
        configure_glue: Some(&|_env: &OverlayEnv, build: &mut cc::Build| {
            build.include(nros_build_paths::nros_lan9118_lwip_dir().join("include"));
        }),
        ..Overlay::new(
            "nros-board-mps3-an536-freertos",
            "an536.ld",
            "GCC/ARM_CRx_No_GIC",
            &["c/board_an536.c"],
        )
    });
}

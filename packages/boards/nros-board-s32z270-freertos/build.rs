//! Build script for nros-board-s32z270-freertos (phase-372 W2).
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
//! Kernel + port provisioning (phase-372 W3, the licensing seam):
//! `FREERTOS_DIR` / `FREERTOS_PORT` env vars override the defaults. The
//! default port here is the in-tree kernel's `GCC/ARM_CRx_No_GIC` so a clean
//! checkout is LINK-COMPLETE; a hardware consumer points `FREERTOS_DIR` at
//! the NXP FreeRTOS distribution and `FREERTOS_PORT` at `GCC/ARM_CR52_GIC`
//! (with the Thumb-resume CPSR patch applied — see phase-372).

use nros_board_common::freertos_build::{Overlay, run_overlay};

fn main() {
    run_overlay(&Overlay::new(
        "nros-board-s32z270-freertos",
        "s32z270_rtu.ld",
        "GCC/ARM_CRx_No_GIC",
        &["c/board_s32z270.c"],
    ));
}

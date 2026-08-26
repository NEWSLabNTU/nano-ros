//! QEMU MPS3-AN536 (dual Cortex-R52) board crate for nros on FreeRTOS + lwIP
//! (phase-385).
//!
//! The emulated counterpart to `nros-board-s32z270-freertos`. Same CPU, same
//! kernel port, same toolchain — but this one RUNS: QEMU models a GICv3, the
//! ARM generic timer and a LAN9118, so `c/board_an536.c` implements the tick
//! seam and the netif that the S32Z270 bundle leaves as fail-loud weak stubs
//! for hardware. That makes this board the place where the ARMv8-R half of the
//! FreeRTOS platform is actually exercised.
//!
//! The C surface (startup, EL2→EL1 drop, GICv3, tick, netif) lives in
//! `c/board_an536.c`; the generic FreeRTOS family glue — including the
//! semihosting console — comes from `nros-board-freertos`. This Rust side is
//! deliberately minimal: the consumer this board exists for (ASI) drives the
//! C/C++ workspace lane (`FreertosBoard::run_tiers` from `nros/main.hpp`).

#![no_std]

// Board crates link the platform even when no Rust symbol is referenced —
// the staticlib DCE rule (issues 0155/0163).
extern crate nros_platform as _;

#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

pub use nros_board_freertos::{BaseConfig, Config};

/// Marker type for the board, mirroring `Mps2An385` and `S32z270Rtu`.
pub struct Mps3An536;

//! NXP S32Z270 RTU (Cortex-R52) board crate for nros on FreeRTOS + lwIP.
//!
//! The C surface (startup, weak netif/tick hooks) lives in `c/board_s32z270.c`;
//! the generic FreeRTOS family glue comes from `nros-board-freertos`. This Rust
//! side is deliberately minimal: the consumer this board exists for (ASI)
//! drives the C/C++ workspace lane (`FreertosBoard::run_sched_tasks` from
//! `rclcpp/detail/main.hpp`), not a Rust entry. A Rust entry lane (console
//! writer, panic behaviour) lands when hardware answers what the console is
//! (LinFlexD via RTD — licensed, so likely another weak-hook seam).

#![no_std]

// Board crates link the platform even when no Rust symbol is referenced —
// the staticlib DCE rule (issues 0155/0163).
extern crate nros_platform as _;

#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

pub use nros_board_freertos::{BaseConfig, Config};

/// Marker type for the board, mirroring `Mps2An385`.
pub struct S32z270Rtu;

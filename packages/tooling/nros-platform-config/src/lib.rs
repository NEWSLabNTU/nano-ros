//! The RFC-0049 knob ladder — `builtin < platform < board < env`.
//!
//! Extracted from `nros-board-common` (phase-400 W6). The reader itself never
//! needed to live in a board crate; it needed to be reachable from every build
//! script that resolves a knob, and the board crate's own dependency on
//! `nros-platform` made that impossible for anything the platform layer
//! reaches. See this crate's `Cargo.toml` for the cycle.
//!
//! `nros-board-common` re-exports both modules, so consumers spelling
//! `nros_board_common::platform_config::…` keep working unchanged.

pub mod manifest;
pub mod platform_config;

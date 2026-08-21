//! Entry pkg for the RT-tiers Rust workspace on FreeRTOS / QEMU MPS2-AN385.
//!
//! issue 0636 gap 2 — the tiers-on-FreeRTOS projection of `realtime-rust`, and
//! the FIRST consumer of the Rust FreeRTOS multi-tier path at all.
//! `run_tiers_entry` has been exported from `nros-board-freertos` and reachable
//! from the macro (its own "no run_tiers" diagnostic lists the freertos family)
//! with nothing calling it: every FreeRTOS realtime fixture was C or C++, and
//! the one Rust FreeRTOS entry (`workspaces/rust`) is single-tier `run_entry`.
//!
//! Same one-line `nros::main!` as the native / nuttx / threadx siblings.
//! `deploy = "freertos"` (Cargo.toml) selects the MPS2-AN385 board, and the
//! `[tiers.*.freertos]` sub-tables in `demo_bringup/system.toml` flip the
//! macro's generic OwnedSpin arm onto `<Mps2An385Freertos>::run_tiers`: one
//! FreeRTOS task per tier over ONE shared zenoh session.
//!
//! `resolve_tiers` sorts descending by RAW priority with no per-RTOS
//! inversion, and FreeRTOS is bigger-is-more-urgent, so the BOOT tier is `low`
//! (telem, 100 ms) and `high` (ctrl, 10 ms) is chain-spawned after telem's
//! setup. That ordering is the whole point of the cell: it is the arrangement
//! issue 0636 fixed on this board, and nothing exercised it in Rust.

#![no_std]
#![no_main]

extern crate panic_semihosting;

nros::main!(panic = "own", launch = "demo_bringup");

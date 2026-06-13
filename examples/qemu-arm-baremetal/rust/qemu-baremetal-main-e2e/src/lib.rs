//! Phase 244.D1 — Entry-pkg lib companion.
//!
//! `nros::main!()` (Form 1 self-bringup) emits
//! `::qemu_baremetal_main_e2e::register(runtime)` — the macro resolves the
//! current pkg name and dispatches to its lib crate's `register` symbol. The
//! application logic lives in the sibling `qemu_baremetal_e2e_pkg` Node pkg
//! (whose `nros::node!` emits `pub fn register(runtime)`); re-export it here.

#![no_std]

pub use qemu_baremetal_e2e_pkg::register;

//! Entry-pkg lib companion.
//!
//! `nros::main!()` (Form 1 self-bringup) emits `::qemu_serial_talker::register(
//! runtime)` — the macro resolves the current pkg name and dispatches to its lib
//! crate's `register` symbol. The application logic lives in the sibling
//! `serial_talker_pkg` Node pkg (whose `nros::node!` emits
//! `pub fn register(runtime)`); re-export it here.

#![no_std]

pub use serial_talker_pkg::register;

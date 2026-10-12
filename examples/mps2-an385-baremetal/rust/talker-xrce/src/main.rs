//! XRCE (custom UART transport) talker entry for QEMU MPS2-AN385 (phase-244.D1).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[image.*] board = "qemu-mps2-an385"`, resolves the
//! bare-metal board, and emits the `#[cortex_m_rt::entry]` boot scaffold. The
//! `system.toml`'s `rmw = "xrce"` + `transport = "serial"` reach the board
//! through the deploy overlay, and `BoardEntry::setup_transport` (the board,
//! built `xrce-transport`) installs the XRCE-over-UART vtable and then
//! registers the XRCE backend — the ordering `set_custom_transport_ops` needs
//! (issue 1601: it keyed on `transport = "xrce"`, which W6 retired). Node logic lives in
//! `xrce_talker_pkg`. No hand-written `xrce::set_custom_transport_ops` /
//! `xrce::register` / `Executor::open` ceremony.

#![no_std]
#![no_main]

// The panic handler is the board's crate-root item (`[board.entry]
// crate_root_extra`), carried by the generated selection package (issue 1305).

nros::main!(panic = "own");

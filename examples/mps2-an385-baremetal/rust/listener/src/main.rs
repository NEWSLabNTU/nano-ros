//! Bare-metal `nros::main!()` BoardEntry listener for QEMU MPS2-AN385.
//!
//! The whole boot scaffold (reset vector via `#[cortex_m_rt::entry]`, RMW
//! register, `Executor::open`, `RuntimeCtx`, node registration, spin) is
//! emitted by `nros::main!()` from `[image.*] board =
//! \"qemu-mps2-an385"`. The only hand-written lines are the bare-metal attrs +
//! panic handler. The application logic lives in the sibling `listener_pkg`
//! Node pkg (re-exported via src/lib.rs).

#![no_std]
#![no_main]

// The panic handler is the board's crate-root item (`[board.entry]
// crate_root_extra`), carried by the generated selection package (issue 1305).

nros::main!(panic = "own");

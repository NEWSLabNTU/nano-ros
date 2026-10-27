//! Phase 244.D1 — bare-metal `nros::main!()` BoardEntry E2E entry pkg.
//!
//! The whole boot scaffold (reset vector via `#[cortex_m_rt::entry]`, RMW
//! register, `Executor::open`, `RuntimeCtx`, node registration, spin) is
//! emitted by `nros::main!()` from `[image.*] board =
//! \"mps2-an385-baremetal"` + `node_pkgs`. The only hand-written lines are the
//! bare-metal attrs; the panic handler is the board's crate-root provider,
//! linked by the generated selection package (issue 1305).

#![no_std]
#![no_main]

nros::main!(panic = "own");

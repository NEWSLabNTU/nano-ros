//! [`BoardExit`] — Phase 212.N.1.
//!
//! Per-board termination contract. Mirrors the legacy
//! `nros-board-common::board_init::BoardExit`.
//!
//! Implementations:
//! - QEMU boards → `cortex_m_semihosting::debug::exit(EXIT_*)`.
//! - Real hardware → reset chip / halt in `wfi` / signal watchdog.
//! - POSIX → `std::process::exit(code)`.
//! - RTOS native sim (Zephyr / NuttX native_sim) → kernel-specific
//!   shutdown then `_exit`.
//!
//! Both methods diverge (`-> !`) because `BoardEntry::run`'s body is
//! `-> Result<…>` only inside the `setup` callback; the outer
//! lifecycle never returns to the caller of `run`.

/// Per-board termination contract.
pub trait BoardExit {
    /// Terminate cleanly after the user closure returned `Ok`.
    fn exit_success() -> !;

    /// Terminate after the user closure returned `Err` or an init
    /// step failed.
    fn exit_failure() -> !;
}

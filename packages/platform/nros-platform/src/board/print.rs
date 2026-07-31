//! [`BoardPrint`] — Phase 212.N.1.
//!
//! Per-board stdout contract. Mirrors the legacy
//! `nros-board-common::board_init::BoardPrint`.
//!
//! Implementing boards wrap one of:
//! - `cortex_m_semihosting::hprintln!` (QEMU Cortex-M / MPS2-AN385)
//! - Vendor printf bridge (orin-spe `tcu_print_msg`)
//! - Serial UART writer
//! - `libc::write(STDOUT_FILENO, …)` (POSIX)
//! - `printk` (Zephyr / NuttX / FreeRTOS-with-stdio)
//!
//! The signature takes `core::fmt::Arguments` so the generic
//! `BoardEntry::run` body can pass `format_args!(...)` through
//! without forcing an allocation or fixed-size buffer at the trait
//! level — each board picks its staging strategy.

/// Per-board stdout contract.
pub trait BoardPrint {
    /// Emit `args` followed by a newline.
    fn println(args: core::fmt::Arguments<'_>);
}

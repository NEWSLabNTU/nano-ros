//! # `nros-board-zephyr` — Zephyr family driver (Phase 212.N.2)
//!
//! Zephyr is the **carve-out** in the Phase 212.N Board trait family:
//! Kconfig + DTS already own the BSP, and Zephyr's build system emits
//! the C `main()` entry point. A Rust staticlib (the only shape
//! `zephyr-lang-rust` supports) cannot take that `main` over from
//! Zephyr, so the usual `<Board as BoardEntry>::run(setup)` shape
//! does not apply here.
//!
//! This crate therefore implements **only [`NetworkWait`]** over
//! `<zephyr/net/net_if.h>`. The user app's Rust `extern "C" fn main`
//! (declared from the Zephyr Rust app template) calls
//! [`ZephyrBoard::wait_link_up`] before opening any RMW session and
//! bails out early on `Err`:
//!
//! ```ignore
//! use nros_board_zephyr::ZephyrBoard;
//! use nros_platform::board::NetworkWait;
//!
//! #[no_mangle]
//! pub extern "C" fn rust_main() {
//!     if ZephyrBoard::wait_link_up().is_err() {
//!         // log + bail — Zephyr's main returns to the kernel.
//!         return;
//!     }
//!     // ... open RMW session, run executor ...
//! }
//! ```
//!
//! ## Why a no-op [`BoardInit`] / [`BoardPrint`] / [`BoardExit`]?
//!
//! The 212.N.1 trait surface declares `NetworkWait: super::Board` —
//! i.e. any `NetworkWait` impl carries the full
//! `BoardInit + BoardPrint + BoardExit` super-trait. Two options:
//!
//! - **(a)** Provide trivial no-op impls of `BoardInit` / `BoardPrint`
//!   / `BoardExit` for `ZephyrBoard` in *this* crate. Documented as
//!   "unused — Zephyr's `main` handles hardware init, printk, and
//!   exit; these exist only to satisfy the trait bound."
//! - **(b)** Loosen `NetworkWait` in `nros-platform` to drop the
//!   `: super::Board` super-trait.
//!
//! We pick **(a)**: a local, contained workaround that does not
//! ripple into `nros-platform` and leaves room for a future Zephyr
//! `BoardEntry` story (e.g. Phase 212.N.7 once the legacy zephyr-rust
//! main wrapper is replaced).
//!
//! ## Build surface
//!
//! `cargo check` on the host (no `ZEPHYR_BASE` required): the Zephyr
//! C symbols are declared via plain `extern "C"` blocks. At link
//! time (cross-compile against the Zephyr build), the Zephyr kernel
//! provides `net_if_get_default`, `net_if_is_up`, and `k_msleep`.
//!
//! A host `cargo build` will fail to link (those symbols are
//! unresolved on a vanilla host) — this is expected; the crate is
//! consumed as a Rust staticlib by Zephyr's `rust_cargo_application`
//! cmake function.

#![no_std]

use core::ffi::c_void;

use nros_platform::{BoardExit, BoardInit, BoardPrint, NetworkError, NetworkWait};

/// Zephyr family board marker.
///
/// Implements [`NetworkWait`] only. The mandatory super-trait
/// (`BoardInit` + `BoardPrint` + `BoardExit`) is satisfied by
/// no-op stubs — see crate-level docs for the rationale.
pub struct ZephyrBoard;

// issue #128 (half 2) — per-tier multi-task entry (`ZephyrBoard::run_tiers`).
// Gated so NetworkWait-only consumers keep the zero-dep footprint; the
// `nros::main!` Zephyr arm's multi-tier emit requires the entry crate to
// enable `tiers` on this board dep.
#[cfg(feature = "tiers")]
mod entry_tiers;

/// Kernel sleep for in-crate callers (the tier tasks' fault parking loop).
/// Routes through the module shim `nros_zephyr_msleep` — `k_msleep` itself
/// is a header-inline/syscall wrapper with no reliably linkable symbol.
#[cfg(feature = "tiers")]
pub(crate) fn zephyr_msleep(ms: i32) {
    unsafe extern "C" {
        fn nros_zephyr_msleep(ms: i32) -> i32;
    }
    // SAFETY: plain kernel sleep — no invariants for Rust to uphold.
    unsafe {
        let _ = nros_zephyr_msleep(ms);
    }
}

// -- Zephyr C surface ---------------------------------------------------------
//
// Hand-rolled `extern "C"` decls to avoid pulling the
// `zephyr-lang-rust` workspace into this crate's build graph (it
// requires `ZEPHYR_BASE` and a configured build dir). When linked
// into a Zephyr app these resolve against `<zephyr/net/net_if.h>`
// and `<zephyr/kernel.h>`.
//
// Signature choices:
// - `net_if_get_default` — returns `struct net_if *`; we treat it as
//   `*mut c_void` (opaque from Rust's POV).
// - `net_if_is_up` — Zephyr's actual signature returns `bool`
//   (`stdbool.h`); we mirror that.
// - `k_msleep` — Zephyr's signature is `int32_t k_msleep(int32_t
//   ms)`; we drop the return value (caller doesn't need it).
unsafe extern "C" {
    fn net_if_get_default() -> *mut c_void;
    fn net_if_is_up(iface: *mut c_void) -> bool;
    fn k_msleep(ms: i32) -> i32;
}

/// Poll interval between `net_if_is_up` checks.
const POLL_INTERVAL_MS: i32 = 100;

/// Total budget for link-up (covers carrier detection + DHCP lease).
const LINK_UP_DEADLINE_MS: i32 = 30_000;

impl NetworkWait for ZephyrBoard {
    /// Block until the default `net_if` reports link-up, polling
    /// every [`POLL_INTERVAL_MS`] for up to [`LINK_UP_DEADLINE_MS`].
    ///
    /// Returns:
    /// - `Ok(())` once `net_if_is_up()` returns true.
    /// - `Err(NetworkError::ConfigInvalid)` if the default `net_if`
    ///   is NULL (no interface registered).
    /// - `Err(NetworkError::DhcpTimeout)` on deadline-miss.
    ///
    /// Note: this checks PHY/carrier readiness, not strictly DHCP
    /// completion — Zephyr's `net_if_is_up` flips once the iface is
    /// admin-up and the L2 driver reports link. A board that boots
    /// with a static IP is reported "up" immediately by the same
    /// API, so the deadline-miss variant maps onto the closest
    /// `NetworkError` flavour (`DhcpTimeout`).
    fn wait_link_up() -> Result<(), NetworkError> {
        // SAFETY: Zephyr's `net_if_get_default` is callable from any
        // thread context post-kernel-init; the returned pointer is
        // either NULL or stable for the lifetime of the kernel.
        let iface = unsafe { net_if_get_default() };
        if iface.is_null() {
            return Err(NetworkError::ConfigInvalid);
        }

        let mut elapsed: i32 = 0;
        while elapsed < LINK_UP_DEADLINE_MS {
            // SAFETY: `iface` is non-NULL (checked above) and Zephyr
            // guarantees `net_if_is_up` is safe to call concurrently
            // with kernel network bring-up.
            if unsafe { net_if_is_up(iface) } {
                return Ok(());
            }
            // SAFETY: `k_msleep` is a plain kernel sleep — no
            // invariants for Rust to uphold.
            unsafe {
                k_msleep(POLL_INTERVAL_MS);
            }
            elapsed = elapsed.saturating_add(POLL_INTERVAL_MS);
        }
        Err(NetworkError::DhcpTimeout)
    }
}

// -- Trivial super-trait impls (unused; satisfy the trait bound) -------------
//
// Zephyr owns `main` and the boot/print/exit lifecycle, so these are
// never invoked through this crate. They exist solely so the
// `NetworkWait: Board` super-trait bound in `nros-platform` is
// satisfied for `ZephyrBoard`.

impl BoardInit for ZephyrBoard {
    /// Unused on Zephyr — `main` already ran kernel + driver init by
    /// the time user code calls into this crate.
    #[inline]
    fn init_hardware() {}
}

impl BoardPrint for ZephyrBoard {
    /// Unused on Zephyr — user code goes through `printk` /
    /// `LOG_INF` directly from the Zephyr app side.
    #[inline]
    fn println(_args: core::fmt::Arguments<'_>) {}
}

impl BoardExit for ZephyrBoard {
    /// Unused on Zephyr — `main` returns to the kernel; there is no
    /// "exit". Hang to satisfy the `-> !` return type.
    fn exit_success() -> ! {
        loop {
            // SAFETY: same as `k_msleep` above.
            unsafe {
                k_msleep(i32::MAX);
            }
        }
    }

    /// Unused on Zephyr — see [`Self::exit_success`].
    fn exit_failure() -> ! {
        loop {
            // SAFETY: same as `k_msleep` above.
            unsafe {
                k_msleep(i32::MAX);
            }
        }
    }
}

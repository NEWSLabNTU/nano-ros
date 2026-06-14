//! Tier-1 per-board shim for hosted Linux / macOS — Phase 212.N.3.
//!
//! `NativeBoard` is a ZST that implements the [`nros_platform`] `Board`
//! trait surface (`BoardInit`, `BoardPrint`, `BoardExit`, `BoardEntry`)
//! by delegating one-for-one to `nros_board_posix::PosixBoard` (the
//! Phase 212.N.2 family driver). There is nothing exotic about the
//! "native" target — libstd's runtime is the entire clock / heap /
//! stdio / threading story, and the POSIX family driver already
//! captures that — so this crate is a thin shim that exists only to
//! give the tier-1 board name (`native`) a dedicated crate the way the
//! embedded targets do (`nros-board-mps2-an385-freertos`,
//! `nros-board-stm32f4`, …).
//!
//! ## Why a separate crate at all?
//!
//! The 212.N.3 spec is "one crate per board". A native Entry pkg
//! could in principle depend on `nros-board-posix` directly, but
//! routing through `nros-board-native::NativeBoard` keeps the codegen
//! emitter (Phase 212.N.4) uniform: `generate_single_node_main(<plat>)`
//! always names a per-board ZST, never a family-driver ZST. If a
//! future native target ever needs a knob the POSIX family doesn't
//! expose (a host-specific clock source, an alternate stdout sink),
//! the override lives here without touching every Entry pkg.
//!
//! ## Plugging it
//!
//! ```ignore
//! use nros_board_native::NativeBoard;
//! use nros_platform::BoardEntry;
//!
//! fn main() {
//!     let _ = <NativeBoard as BoardEntry>::run(|runtime| {
//!         // codegen-emitted (Phase 212.N.4)
//!         run_plan(runtime)
//!     });
//! }
//! ```

#![forbid(unsafe_op_in_unsafe_fn)]

// Phase 248 C5a (#60 T4) — the BOARD is the RMW selection point. Under its
// own `rmw-zenoh` feature it force-links the zenoh backend rlib so the
// backend's `RMW_INIT_ENTRIES` self-register section survives stable-Rust
// rlib pruning and reaches the final binary, WITHOUT relying on the `nros`
// umbrella's `rmw-zenoh` feature. Mirrors the `__FORCE_LINK_ZENOH` static in
// `nros/src/lib.rs` (referencing `register` keeps both the symbol and its
// linker section alive — strictly stronger than the prior `extern crate _`,
// which only kept the rlib). On native (linkme-aware + `.init_array`) the
// section auto-registers; the static guarantees it is not pruned first.
// Cycle-free: the backend crate does not depend on this board crate. Inert
// unless `rmw-zenoh` selects the backend.
#[cfg(feature = "rmw-zenoh")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;

use nros_board_posix::PosixBoard;
use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx, TierSpec};

/// Tier-1 per-board shim. See the crate-level docs for the rationale
/// behind shipping a dedicated ZST rather than re-exporting
/// `PosixBoard` directly.
pub struct NativeBoard;

impl BoardInit for NativeBoard {
    #[inline]
    fn init_hardware() {
        <PosixBoard as BoardInit>::init_hardware()
    }
}

impl BoardPrint for NativeBoard {
    #[inline]
    fn println(args: core::fmt::Arguments<'_>) {
        <PosixBoard as BoardPrint>::println(args)
    }
}

impl BoardExit for NativeBoard {
    #[inline]
    fn exit_success() -> ! {
        <PosixBoard as BoardExit>::exit_success()
    }

    #[inline]
    fn exit_failure() -> ! {
        <PosixBoard as BoardExit>::exit_failure()
    }
}

/// Phase 249 P3.5 — the board owns the RMW registration on EVERY OS (hosted
/// included), not just `target_os="none"`. Calling the linked backend's
/// `register()` explicitly before the executor opens is the one universal
/// trigger; it replaces the hosted reliance on the linkme `.init_array` walk
/// (retired in P4) and lets the weak `nros_app_register_backends` default die.
/// Gated on the board's own `rmw-<x>` feature; the call also force-links the
/// backend (strictly stronger than the `#[used] __FORCE_LINK_*` static). Inert
/// when no backend feature is selected.
#[inline]
fn register_backend() {
    #[cfg(feature = "rmw-zenoh")]
    {
        let _ = nros_rmw_zenoh::register();
    }
    #[cfg(feature = "rmw-xrce")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        let _ = nros_rmw_cyclonedds_sys::register();
    }
}

impl BoardEntry for NativeBoard {
    /// One-line delegation to the POSIX family driver. The lifecycle
    /// (`init_hardware` → build `RuntimeCtx` → invoke `setup` →
    /// `exit_*`) lives in `PosixBoard::run`; see the docs there. Phase 249
    /// P3.5: the board registers its linked backend first (all OSes).
    #[inline]
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        register_backend();
        <PosixBoard as BoardEntry>::run::<F, E>(setup)
    }
}

impl NativeBoard {
    /// Phase 228.G — per-tier multi-task entry; delegates to
    /// [`PosixBoard::run_tiers`]. The `nros::main!()` proc-macro emits
    /// `<NativeBoard>::run_tiers(TIERS, run_plan)` for multi-tier systems
    /// (single-tier keeps the `BoardEntry::run` path). See `nros-board-posix`.
    #[inline]
    pub fn run_tiers<F, E>(
        deploy: &nros_platform::DeployOverlay,
        tiers: &[TierSpec<'_>],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E> + Sync,
        E: core::fmt::Debug,
    {
        // Phase 249 P3.5 — register the linked backend before the tiers open.
        register_backend();
        // Issue #48 — hosted boards take their locator from the environment, so
        // the deploy overlay is a no-op; forwarded for signature parity.
        PosixBoard::run_tiers::<F, E>(deploy, tiers, setup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_hardware_is_noop() {
        // Smoke: delegating `init_hardware` must not panic.
        <NativeBoard as BoardInit>::init_hardware();
    }

    #[test]
    fn println_writes_without_panicking() {
        <NativeBoard as BoardPrint>::println(format_args!(
            "nros-board-native: hello from unit test"
        ));
    }

    // `BoardEntry::run` itself can't be unit-tested directly — both
    // exit branches diverge (`-> !`) via `std::process::exit`, which
    // would tear down the test process. The non-diverging `setup`
    // callback path is the test seam, identical to `nros-board-posix`.
}

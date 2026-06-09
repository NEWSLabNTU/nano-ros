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

// Phase 212.N.7 step-3.5 — force-link the zenoh RMW backend so its
// `.nros_rmw_init` linker-section ctor reaches the final binary.
// Without this `extern crate _`, cargo drops the rlib at link time
// (the rest of the crate never names a zenoh symbol), and
// `Executor::open` (now invoked inside `PosixBoard::run`) finds no
// backend on first call.
extern crate nros_rmw_zenoh as _;

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

impl BoardEntry for NativeBoard {
    /// One-line delegation to the POSIX family driver. The lifecycle
    /// (`init_hardware` → build `RuntimeCtx` → invoke `setup` →
    /// `exit_*`) lives in `PosixBoard::run`; see the docs there.
    #[inline]
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        <PosixBoard as BoardEntry>::run::<F, E>(setup)
    }
}

impl NativeBoard {
    /// Phase 228.G — per-tier multi-task entry; delegates to
    /// [`PosixBoard::run_tiers`]. The `nros::main!()` proc-macro emits
    /// `<NativeBoard>::run_tiers(TIERS, run_plan)` for multi-tier systems
    /// (single-tier keeps the `BoardEntry::run` path). See `nros-board-posix`.
    #[inline]
    pub fn run_tiers<F, E>(tiers: &[TierSpec<'_>], setup: F) -> Result<(), E>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E> + Sync,
        E: core::fmt::Debug,
    {
        PosixBoard::run_tiers::<F, E>(tiers, setup)
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

//! [`BoardEntry`] — Phase 212.N.1.
//!
//! The single boot-driver trait every Entry pkg `main.rs` invokes:
//!
//! ```ignore
//! fn main() {
//!     let _ = <MyBoard as BoardEntry>::run(|runtime| {
//!         run_plan(runtime)         // codegen-emitted (212.N.4)
//!     });
//! }
//! ```
//!
//! `run` owns the full lifecycle:
//!
//! 1. [`super::BoardInit::init_hardware`]
//! 2. [`super::TransportBringup::init_transport`] (if implemented)
//! 3. [`super::NetworkWait::wait_link_up`] (if implemented)
//! 4. Open executor, build `RuntimeCtx`, invoke `setup(runtime)`.
//! 5. Spin executor to completion (or termination signal).
//! 6. [`super::BoardExit::exit_success`] / `exit_failure`.
//!
//! The exact body lives in the family driver crates (212.N.2); the
//! trait here pins the signature so codegen + user Entry pkg can
//! call it without knowing the family.

use super::runtime::RuntimeCtx;

/// Per-board boot driver.
///
/// Implementations live in the family driver crates
/// (`nros-board-posix`, `nros-board-freertos`, …). Per-board crates
/// (`nros-board-mps2-an385-freertos`, …) plug the family.
pub trait BoardEntry: super::Board {
    /// Drive the full boot → user-closure → exit flow.
    ///
    /// `setup` receives a `&mut RuntimeCtx` with overlay knobs from
    /// the launch file / CLI args. Returning `Err` from `setup` makes
    /// `run` route to [`super::BoardExit::exit_failure`]; `Ok`
    /// proceeds to executor spin + clean exit.
    ///
    /// **Returns `Result`, not `!`.** The legacy
    /// `nros-board-common::board_init::BoardEntry::run` diverged;
    /// 212.N keeps the option to return so unit tests can drive it
    /// in a hosted process without `exit()` killing the test
    /// harness. Production boards still call `exit_*` from inside
    /// `run`'s body after spin returns.
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug;
}

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

/// Deploy-metadata overlay threaded from `nros::main!()` into the board's
/// boot config (issue #48 cause 1).
///
/// The `nros::main!()` macro reads the Entry pkg's
/// `[package.metadata.nros.deploy.<board>]` block at expansion time and bakes
/// the present keys here. Each field is `None` when the deploy block omitted
/// it, so the board overlays only the supplied values onto its own
/// `Config::default()` (the firmware's compiled-in default stays the source of
/// truth for everything the deploy block does not name).
///
/// Boards whose `BoardEntry::run` ignores network/locator config (POSIX hosts,
/// RTIC/Embassy MCUs that take their transport elsewhere) inherit the default
/// [`BoardEntry::run_with_deploy`] body, which drops the overlay and calls
/// [`BoardEntry::run`] — so adding fields here never touches those boards.
#[derive(Clone, Copy, Default, Debug)]
pub struct DeployOverlay {
    /// `locator = "tcp/10.0.2.2:7451"` — the zenoh/RMW endpoint the firmware
    /// dials. `None` → keep the board default.
    pub locator: Option<&'static str>,
    /// `ip = "10.0.2.15"` — static guest IP. `None` → keep the board default.
    pub ip: Option<[u8; 4]>,
    /// `gateway = "10.0.2.2"` — default route. `None` → keep the board default.
    pub gateway: Option<[u8; 4]>,
    /// `netmask = "255.255.255.0"`. `None` → keep the board default.
    pub netmask: Option<[u8; 4]>,
    /// `domain_id = 0` — ROS 2 domain. `None` → keep the board default.
    pub domain_id: Option<u32>,
}

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

    /// Boot like [`run`](Self::run) but apply a deploy-metadata overlay to the
    /// board's boot config first (issue #48 cause 1).
    ///
    /// The default body **ignores** `deploy` and forwards to
    /// [`run`](Self::run); boards that compile a network/locator config (the
    /// FreeRTOS / bare-metal firmware boards) override it to overlay the
    /// supplied fields onto their `Config::default()`. `nros::main!()` calls
    /// this (not `run`) for `target_os = "none"` OwnedSpin targets so the
    /// `[package.metadata.nros.deploy.<board>]` block stops being inert.
    fn run_with_deploy<F, E>(_deploy: &DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        Self::run(setup)
    }
}

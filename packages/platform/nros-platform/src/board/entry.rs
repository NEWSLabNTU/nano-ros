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
/// [`BoardEntry::run`] — so adding a *network* field here never touches those
/// boards. The exception is [`node_name`](DeployOverlay::node_name): hosted
/// boards override `run_with_deploy` to apply it to the boot config (issue #98),
/// since the ROS graph node name is a launch identity, not a network knob.
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
    /// `transport = "xrce"` — select a board custom transport that must be
    /// installed BEFORE the linked RMW registers (e.g. an XRCE-over-UART vtable).
    /// `None` → the board's default transport. Honored by
    /// [`BoardEntry::setup_transport`] (phase-244.D1).
    pub transport: Option<&'static str>,
    /// The ROS graph node name for the primary session, baked from the launch
    /// file's single `<node name=…>` / `system.toml` `[[component]].name` (issue
    /// #98). `None` → the board default (`from_env()`'s `"node"`). Only set by
    /// `nros::main!` when the launch declares exactly one node — multiple nodes
    /// share one primary session, so naming it after one component would be
    /// wrong (per-node naming is the deferred multi-node piece). Applied to the
    /// boot `ExecutorConfig` by the board, so unlike `locator` this IS honored on
    /// hosted boards (locator stays env-driven; node name is a launch identity).
    pub node_name: Option<&'static str>,
    /// Issue #101 / RFC-0045 — the patchable baked boot-config static
    /// (`.nros_boot_config`), emitted by `nros::main!` for embedded targets and
    /// read by the board to resolve node_name/locator/domain. `None` on hosted /
    /// when the macro emits no static.
    pub boot_config: Option<&'static nros_platform_api::BakedBootConfig>,
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

    /// phase-271 (issue #110) — boot like [`run_with_deploy`](Self::run_with_deploy)
    /// but size the executor's callback table + arena to the entry's OWN declared
    /// topology (`max_cbs` / `max_sched_contexts`, from the entry's
    /// `[package.metadata.nros.entry]`), instead of the workspace-global
    /// `NROS_EXECUTOR_MAX_CBS` build const.
    ///
    /// Sizes are plain `usize`s (not `nros::ExecutorSizing`) because
    /// `nros-platform` sits below `nros`; the hosted board converts them. A
    /// `max_sched_contexts` of `0` means "use the build default". The **default
    /// body IGNORES the sizing** and forwards to
    /// [`run_with_deploy`](Self::run_with_deploy), so every board except the
    /// hosted (posix) one — which opens via `Executor::open` and could grow its
    /// arena — is byte-identical; the posix board overrides this to
    /// `Executor::open_sized`. `nros::main!()` emits this (instead of
    /// `run_with_deploy`) only when the entry declares `max_callbacks`.
    fn run_with_deploy_sized<F, E>(
        deploy: &DeployOverlay,
        _max_cbs: usize,
        _max_sched_contexts: usize,
        setup: F,
    ) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        Self::run_with_deploy(deploy, setup)
    }

    /// **Custom-transport install seam.** Install a board-specific transport
    /// selected by `deploy.transport`, BEFORE the linked RMW registers
    /// (phase-244.D1).
    ///
    /// `nros::main!()` always emits a `setup_transport` call (gated on
    /// `target_os = "none"`) immediately before `__register_linked_rmw()`,
    /// so that the vtable is in place before the XRCE backend registers —
    /// the ordering `set_custom_transport_ops` requires.
    ///
    /// **This method is intentionally kept** — it is not dead code. The
    /// **default no-op** is correct for every board whose transport is
    /// registered automatically (Zenoh, native sockets, etc.). The only
    /// current override is **`nros-board-mps2-an385`** with the
    /// `xrce-transport` feature, which installs an XRCE-over-UART vtable
    /// when `deploy.transport == Some("xrce")`. Future boards that need to
    /// pre-register a custom transport vtable should override this method in
    /// the same pattern.
    ///
    /// Failures are the board's to handle (it owns `exit_failure`).
    fn setup_transport(_deploy: &DeployOverlay) {}
}

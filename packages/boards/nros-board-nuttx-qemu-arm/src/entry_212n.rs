//! Phase 212.N.3 — `nros_platform::Board*` trait impls + `BoardEntry::run`.
//!
//! Mirrors the legacy `nros_board_common::Board{Init,Print,Exit}` impls in
//! [`crate::QemuArmVirt`] onto the new platform-level trait family living at
//! `packages/core/nros-platform/src/board/`. Codegen-emitted Entry pkg
//! `main.rs` (Phase 212.N.4) can then call:
//!
//! ```ignore
//! use nros_board_nuttx_qemu_arm::QemuArmVirt;
//! use nros_platform::BoardEntry;
//!
//! fn main() -> Result<(), MyError> {
//!     <QemuArmVirt as BoardEntry>::run(|runtime| {
//!         // codegen-emitted (Phase 212.N.4)
//!         run_plan(runtime)
//!     })
//! }
//! ```
//!
//! ## Lifecycle vs. the legacy `run()`
//!
//! The legacy [`crate::run`] is config-carrying (`fn(Config, FnOnce(&Config))`)
//! — the user hand-rolls a `Config`, passes it in, and the closure sees a
//! `&Config`. The 212.N flow inverts that: codegen owns config plumbing
//! through `RuntimeCtx`, and `init_hardware` becomes parameterless. The
//! per-board crate's `init_hardware()` therefore can't do the
//! [`crate::node::init_hardware`] IP-override step — that step depends on a
//! runtime-loaded `Config`. The new trait body is a documented no-op for
//! 212.N.3; once codegen lands the IP/locator plumbing on `RuntimeCtx`
//! (Phase 212.N.4+), the override moves into the `setup` closure or a
//! `BoardEntry::run` body extension.
//!
//! ## Why this lives in a sibling module
//!
//! `crate::lib.rs` keeps the legacy `nros_board_common::Board*` impls
//! untouched (per Phase 212.N.3 spec: "keep legacy impls untouched"). The new
//! impls and the `BoardEntry::run` body are isolated here so the two trait
//! families coexist during the 212.N transition; `lib.rs` re-exports
//! [`QemuArmVirt`] for the platform-level path.

use crate::QemuArmVirt;

impl nros_platform::BoardInit for QemuArmVirt {
    /// Phase 212.N.3 — no-arg `init_hardware`.
    ///
    /// The legacy `<QemuArmVirt as nros_board_common::BoardInit>` body
    /// re-seeds `/dev/urandom` from `Config.ip` and pushes `Config.ip`
    /// into `eth0` via `SIOCSIFADDR` — both config-dependent. The new
    /// platform-level trait is parameterless (config moves to
    /// `RuntimeCtx`), so the trait body is a documented no-op until
    /// codegen wires the IP override through `RuntimeCtx` in 212.N.4.
    ///
    /// NuttX brings up `eth0` (virtio-net) during kernel boot before
    /// `main`, so on this board there's no hardware-init step that
    /// belongs here independent of `Config`.
    #[inline]
    fn init_hardware() {
        // No-op: config-driven init lives in the legacy
        // `nros_board_common::BoardInit::init_hardware(&Config)` path
        // and will move into `RuntimeCtx` once 212.N.4 codegen lands.
    }
}

impl nros_platform::BoardPrint for QemuArmVirt {
    /// Routes through hosted stdlib — same primitive the legacy
    /// `<QemuArmVirt as nros_board_common::BoardPrint>` impl uses.
    /// NuttX ships `std`; `println!` ultimately bottoms out in
    /// `write(2)` on the NuttX serial console.
    fn println(args: core::fmt::Arguments<'_>) {
        println!("{args}");
    }
}

impl nros_platform::BoardExit for QemuArmVirt {
    /// Mirrors the legacy `nros_board_common::BoardExit` body.
    ///
    /// NuttX's shell task-dispatch loop reclaims the task on a normal
    /// return from `main`, but `BoardEntry::run`'s contract for
    /// non-NuttX siblings diverges via `exit_*`. We keep
    /// `std::process::exit(...)` here so a caller invoking
    /// `<QemuArmVirt as BoardExit>::exit_success()` directly behaves
    /// identically across families.
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}

/// `BoardEntry::run` delegates to [`nros_board_nuttx::run_entry`].
///
/// NuttX is the carve-out documented on
/// [`nros_platform::BoardEntry`]: `run_entry` returns the
/// [`Result`] the `setup` closure produced rather than
/// diverging. The NuttX shell's task dispatcher reclaims the
/// task when `main` returns, so a hosted test harness can drive
/// `run` without `exit()` killing the test process — see the
/// "Why this does not diverge" docs on
/// [`nros_board_nuttx::run_entry`].
///
/// ## cfg gate
///
/// `nros_board_nuttx::run_entry` itself is gated
/// `#[cfg(any(feature = "reference-qemu-arm", target_os = "nuttx"))]`.
/// Enabling `reference-qemu-arm` from this crate would pull
/// `nros-board-nuttx-qemu-arm` (this crate) back as an optional dep,
/// which cargo rejects as a cyclic package dependency. So we mirror
/// the `target_os = "nuttx"` half of the gate here: the `BoardEntry`
/// impl exists only when actually targeting NuttX, which is the only
/// target where `<QemuArmVirt as BoardEntry>::run` is reachable in a
/// production build anyway. Host `cargo check` still validates the
/// `BoardInit` / `BoardPrint` / `BoardExit` impls above.
#[cfg(target_os = "nuttx")]
impl nros_platform::BoardEntry for QemuArmVirt {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        nros_board_nuttx::run_entry::<Self, F, E>(None, setup)
    }

    /// Issue #98 / RFC-0045 — thread the baked boot-config into the NuttX family
    /// driver so the node name comes from the launch-baked `.nros_boot_config`
    /// static rather than the hardcoded `"nros_app"` default. Locator + domain-id
    /// continue to be baked at compile time via `NROS_LOCATOR` / `NROS_DOMAIN_ID`
    /// (the `BAKED_LOCATOR` / `BAKED_DOMAIN` constants in `run_entry`); only the
    /// node-name originates from `deploy.boot_config` here.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        nros_board_nuttx::run_entry::<Self, F, E>(deploy.boot_config, setup)
    }
}

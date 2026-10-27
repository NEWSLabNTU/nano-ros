//! Phase 212.N.3 — `nros_platform::Board*` trait impls + `BoardEntry::run`.
//!
//! Mirrors the legacy `nros_board_common::Board{Init,Print,Exit}` impls in
//! [`crate::NuttxQemu`] onto the new platform-level trait family living at
//! `packages/platform/nros-platform/src/board/`. Codegen-emitted Entry pkg
//! `main.rs` (Phase 212.N.4) can then call:
//!
//! ```ignore
//! use nros_board_nuttx_qemu::NuttxQemu;
//! use nros_platform::BoardEntry;
//!
//! fn main() -> Result<(), MyError> {
//!     <NuttxQemu as BoardEntry>::run(|runtime| {
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
//! through `RuntimeCtx`, and `init_hardware` becomes parameterless. On NuttX
//! that costs nothing: the kernel owns the network (RFC-0109 D2), and the
//! locator/domain the session needs come from the deploy overlay.
//!
//! ## Why this lives in a sibling module
//!
//! `crate::lib.rs` keeps the legacy `nros_board_common::Board*` impls
//! untouched (per Phase 212.N.3 spec: "keep legacy impls untouched"). The new
//! impls and the `BoardEntry::run` body are isolated here so the two trait
//! families coexist during the 212.N transition; `lib.rs` re-exports
//! [`NuttxQemu`] for the platform-level path.

use crate::NuttxQemu;

impl nros_platform::BoardInit for NuttxQemu {
    /// Phase 212.N.3 — no-arg `init_hardware`.
    ///
    /// NuttX brings up `eth0` (virtio-net) during kernel boot, before
    /// `main`, from its own `defconfig` (RFC-0109 D2), so on this board
    /// there is no hardware-init step that belongs here. (Issue 2048 retired
    /// the `SIOCSIFADDR` push + `/dev/urandom` reseed this comment used to
    /// say would move here.)
    #[inline]
    fn init_hardware() {}
}

impl nros_platform::BoardPrint for NuttxQemu {
    /// phase-359 W7 — routes to the base crate's console, which is `write(2)`
    /// on the NuttX serial console. That is where this always ended up: it went
    /// through libstd's `Stdout` and `LineWriter` first, and now it does not.
    fn println(args: core::fmt::Arguments<'_>) {
        println!("{args}");
    }
}

impl nros_platform::BoardExit for NuttxQemu {
    /// Mirrors the legacy `nros_board_common::BoardExit` body.
    ///
    /// NuttX's shell task-dispatch loop reclaims the task on a normal
    /// return from `main`, but `BoardEntry::run`'s contract for
    /// non-NuttX siblings diverges via `exit_*`. We keep an `exit(...)`
    /// here so a caller invoking
    /// `<NuttxQemu as BoardExit>::exit_success()` directly behaves
    /// identically across families.
    ///
    /// phase-359 W7 — this was `std::process::exit`, whose whole body is a call
    /// to libc `exit(3)`. Same function, same status, one layer fewer.
    fn exit_success() -> ! {
        nros_board_nuttx::sys::exit_process(0)
    }

    fn exit_failure() -> ! {
        nros_board_nuttx::sys::exit_process(1)
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
/// `#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]`.
/// Enabling `reference-qemu` from this crate would pull
/// `nros-board-nuttx-qemu` (this crate) back as an optional dep,
/// which cargo rejects as a cyclic package dependency. So we mirror
/// the `target_os = "nuttx"` half of the gate here: the `BoardEntry`
/// impl exists only when actually targeting NuttX, which is the only
/// target where `<NuttxQemu as BoardEntry>::run` is reachable in a
/// production build anyway. Host `cargo check` still validates the
/// `BoardInit` / `BoardPrint` / `BoardExit` impls above.
#[cfg(target_os = "nuttx")]
impl nros_platform::BoardEntry for NuttxQemu {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let cfg = entry_net_init(None);
        nros_board_nuttx::run_entry::<Self, F, E>(None, cfg.locator(), cfg.domain_id(), setup)
    }

    /// Issue #98 / RFC-0045 — thread the baked boot-config into the NuttX family
    /// driver so the node name comes from the launch-baked `.nros_boot_config`
    /// static rather than the hardcoded `"nros_app"` default.
    ///
    /// Issue 1836 — the locator and domain come from the same overlay: the
    /// image's `system.toml` over the board's default ([`entry_net_init`]).
    /// They used to come from a build-time `NROS_LOCATOR` alone, so an image
    /// that stated its locator only in `system.toml` dialled the empty one.
    ///
    /// Issue 2048 — `eth0` is the kernel's; [`entry_net_init`] only reports it.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let cfg = entry_net_init(Some(deploy));
        nros_board_nuttx::run_entry::<Self, F, E>(
            deploy.boot_config,
            cfg.locator(),
            cfg.domain_id(),
            setup,
        )
    }
}

/// phase-281 W3-nuttx (RFC-0015 Model 1) — the multi-tier inherent entry the
/// `nros::main!` generic OwnedSpin arm targets. For a multi-tier plan the macro
/// emits `<NuttxQemu>::run_tiers(&deploy, TIERS, TIER_BACKING, closure)` (exactly as it does
/// `LinuxBoard::run_tiers` for native); this merges the deploy overlay (the
/// same [`entry_net_init`] the single-tier `run{,_with_deploy}` paths use) and
/// then delegates to the NuttX family driver's
/// [`nros_board_nuttx::run_tiers`], which opens the ONE session and runs one
/// executor per tier over it (a `std::thread` per tier — NuttX is `std` +
/// zenoh-pico `Z_FEATURE_MULTI_THREAD = 1`).
///
/// Gated `target_os = "nuttx"` for the same reason as the [`nros_platform::BoardEntry`]
/// impl below: `entry_net_init` + `nros_board_nuttx::run_tiers` are NuttX-only,
/// and the multi-tier entry is reachable only in a real NuttX build.
#[cfg(target_os = "nuttx")]
impl NuttxQemu {
    pub fn run_tiers<F, E>(
        deploy: &nros_platform::DeployOverlay,
        tiers: &[nros_platform::TierSpec<'_>],
        // issue 1571 — the spawned tiers' executor backing: the entry's
        // `.bss` `TierExecutorBacking`, emitted by `nros::main!`.
        tier_backing: &'static mut [nros_board_nuttx::TierExecutorBackingSlot],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Sync,
        E: core::fmt::Debug,
    {
        let cfg = entry_net_init(Some(deploy));
        nros_board_nuttx::run_tiers::<Self, F, E>(
            deploy.boot_config,
            cfg.locator(),
            cfg.domain_id(),
            tiers,
            tier_backing,
            setup,
        )
    }
}

/// The board config an entry runs with: the board's defaults (`BOARD_NET_*`,
/// generated from the kernel `defconfig`) under the image's deploy overlay.
/// Only its locator and domain are consumed — one overlay for the session
/// (issue 1836).
///
/// Issue 2048 — this also used to push `cfg.ip()` / netmask / gateway into
/// `eth0` with `SIOCSIFADDR` (issue #130). NuttX owns that interface
/// (RFC-0109 D2) and brings it up from the same `defconfig` before `main`, so
/// the push wrote back what the kernel had already applied. The entry now
/// only READS it, and prints it ([`crate::node::report_kernel_eth0`]) so the
/// boot log says which address the session dials from.
#[cfg(target_os = "nuttx")]
fn entry_net_init(deploy: Option<&nros_platform::DeployOverlay>) -> crate::config::Config {
    crate::node::report_kernel_eth0();
    match deploy {
        Some(deploy) => crate::config::Config::default().with_deploy(deploy),
        None => crate::config::Config::default(),
    }
}

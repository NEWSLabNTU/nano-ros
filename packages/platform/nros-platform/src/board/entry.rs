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
//! 2. device bring-up — link layer to L2, then the carrier / DHCP gate if the
//!    board has an IP stack. This is the `run` BODY's job, usually delegated to
//!    a family helper (`nros_board_freertos::run_entry`, …). There is no mixin
//!    trait for it: phase-206 W4 / issue 1067 removed `TransportBringup` and
//!    `NetworkWait`, which had zero and zero production callers respectively,
//!    and whose "skipped if not implemented" order was not expressible in Rust.
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
    /// `[image.<id>] transport = "serial"` — the LINK kind the image rides
    /// (RFC-0086 D2). `None` → the board's default link. Read by
    /// [`BoardEntry::setup_transport`] (phase-244.D1), which may install a
    /// board custom transport for it BEFORE the linked RMW registers.
    ///
    /// Typed, not a string (issue 1601): phase-445 W6 moved the leaf key from
    /// the RMW name (`"xrce"`) to the link kind (`"serial"`), and the one
    /// board reading it kept comparing against `"xrce"` — a comparison that
    /// could no longer be true and compiled anyway. A [`LinkKind`] cannot be
    /// compared against an RMW name.
    pub transport: Option<LinkKind>,
    /// `rmw = "xrce"` — the RMW backend the image declares (`[image.<id>]
    /// rmw` > `[image_defaults] rmw` > `[system] rmw`), verbatim. `None` → not
    /// declared (a Form 2 entry, or a leaf with no `system.toml`).
    ///
    /// The BACKEND choice, which [`transport`](Self::transport) is not: an
    /// XRCE image over a UART states both, and a board that registers a
    /// backend keys on this field (issue 1601).
    pub rmw: Option<&'static str>,
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

impl DeployOverlay {
    /// Does the image declare RMW `rmw` riding link `link`? Both halves are
    /// required: the backend is `rmw`, the link is `[image.<id>] transport`,
    /// and neither stands in for the other (issue 1601).
    pub fn selects(&self, rmw: &str, link: LinkKind) -> bool {
        self.rmw == Some(rmw) && self.transport == Some(link)
    }
}

/// The link kind an image rides — `[image.<id>] transport` (RFC-0086 D2).
///
/// The same three words as `nros_orchestration_ir::leaf_system::TRANSPORT_KINDS`
/// (and `nros_platform_config`'s copy), which `nros::main!` maps onto these
/// variants: an unknown word is refused when the leaf is read, and a word the
/// macro maps to a variant missing here fails the image's compile. It names a
/// LINK, never an RMW — `"xrce"` is not one (issue 1601).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// A byte-stream link (UART, USB-CDC, …).
    Serial,
    /// TCP over the board's IP stack.
    Tcp,
    /// UDP over the board's IP stack.
    Udp,
}

impl LinkKind {
    /// Every link kind, in `TRANSPORT_KINDS` order.
    pub const ALL: [LinkKind; 3] = [LinkKind::Serial, LinkKind::Tcp, LinkKind::Udp];

    /// The `system.toml` spelling.
    pub const fn name(self) -> &'static str {
        match self {
            LinkKind::Serial => "serial",
            LinkKind::Tcp => "tcp",
            LinkKind::Udp => "udp",
        }
    }

    /// Parse the `system.toml` spelling; `None` for anything that is not a
    /// link kind — an RMW name included.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// Per-board boot driver.
///
/// Implementations live in the family driver crates
/// (`nros-board-linux`, `nros-board-freertos`, …). Per-board crates
/// (`nros-board-mps2-an385-freertos`, …) plug the family.
pub trait BoardEntry: super::Board {
    /// Drive the full boot → user-closure → exit flow.
    ///
    /// `setup` receives a `&mut RuntimeCtx` with overlay knobs projected
    /// from the launch file at BUILD time (there is no CLI parse; see
    /// [`super::RuntimeCtx`]). Returning `Err` from `setup` makes
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
    /// hosted (Linux) one — which opens via `Executor::open` and could grow its
    /// arena — is byte-identical; `nros-board-linux` overrides this to
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
    /// selected by `deploy.rmw` + `deploy.transport`, BEFORE the linked RMW registers
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
    /// when the image declares `rmw = "xrce"` over `transport = "serial"`
    /// ([`DeployOverlay::selects`]). Future boards that need to
    /// pre-register a custom transport vtable should override this method in
    /// the same pattern.
    ///
    /// Failures are the board's to handle (it owns `exit_failure`).
    fn setup_transport(_deploy: &DeployOverlay) {}
}

#[cfg(test)]
mod link_kind_tests {
    use super::{DeployOverlay, LinkKind};

    #[test]
    fn link_kinds_round_trip_and_an_rmw_name_is_not_one() {
        for k in LinkKind::ALL {
            assert_eq!(LinkKind::from_name(k.name()), Some(k));
        }
        for rmw in ["xrce", "zenoh", "cyclonedds", "uorb"] {
            assert_eq!(
                LinkKind::from_name(rmw),
                None,
                "`{rmw}` is an RMW, not a link"
            );
        }
    }

    /// Issue 1601: the XRCE-over-UART image declares `rmw = "xrce"` and
    /// `transport = "serial"`; the board's predicate must answer yes to that
    /// pair and no when either half is something else.
    #[test]
    fn selects_needs_both_the_rmw_and_the_link() {
        let xrce_uart = DeployOverlay {
            rmw: Some("xrce"),
            transport: Some(LinkKind::Serial),
            ..Default::default()
        };
        assert!(xrce_uart.selects("xrce", LinkKind::Serial));
        assert!(!xrce_uart.selects("zenoh", LinkKind::Serial));
        assert!(!xrce_uart.selects("xrce", LinkKind::Udp));
        let zenoh_serial = DeployOverlay {
            rmw: Some("zenoh"),
            transport: Some(LinkKind::Serial),
            ..Default::default()
        };
        assert!(!zenoh_serial.selects("xrce", LinkKind::Serial));
        assert!(!DeployOverlay::default().selects("xrce", LinkKind::Serial));
    }
}

//! RFC-0051 / phase-295 W1 — THE test matrix (single source of truth).
//!
//! Every runtime e2e lane in `nros-tests` is a **cell** of this table:
//! (platform × language × RMW × workload × kind). The parametrized matrix
//! consumers (`example_e2e`, `workspace_e2e`, …) iterate [`CELLS`]; the
//! isolation allocator ([`crate::alloc`]) derives each cell's port/domain;
//! the coverage gate cross-checks `examples/fixtures.toml` against this
//! table in BOTH directions. A gap in coverage is a visible
//! [`Tier::BuildOnly`] / [`Tier::CarveOut`] row here — never an absent
//! file (the pre-295 failure mode: nobody can see a test that doesn't
//! exist).
//!
//! Rules:
//! - Carve-outs carry their REASON in the table (audit E5: no
//!   tribal-memory carve-outs).
//! - New platform / language / RMW support adds cells HERE first; the
//!   matrix consumer then runs them without new test files (audit E6).
//! - `Workload` values map 1:1 onto the stock-ROS-demo behavior contracts
//!   the shared checker asserts (audit E7).

use crate::platform::{TestLang, TestVariant};

/// Platform axis. Extends the historical `platform.rs` QEMU set with the
/// native / emulator / hardware targets so the WHOLE lane inventory lives
/// in one axis.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PlatformId {
    /// Host-native (posix). Isolation is EPHEMERAL (ports/domains picked
    /// at runtime) — the allocator's baked formula does not apply.
    Native,
    /// Zephyr native_sim (NSOS host sockets).
    ZephyrNativeSim,
    /// FreeRTOS on QEMU MPS2-AN385 (lwIP).
    FreertosMps2,
    /// NuttX on QEMU arm virt (Cortex-A7).
    NuttxArm,
    /// NuttX on QEMU rv-virt (riscv32).
    NuttxRiscv,
    /// ThreadX Linux simulation (host sockets).
    ThreadxLinux,
    /// ThreadX on QEMU riscv64 virt (NetX Duo).
    ThreadxRiscv64,
    /// ESP32-C3 under the Espressif QEMU fork (open_eth).
    Esp32Qemu,
    /// Bare-metal RTIC on QEMU MPS2-AN385.
    QemuBaremetal,
    /// STM32F4 hardware (NUCLEO-F429ZI) — RTIC + Embassy.
    Stm32F4,
    /// ARM FVP Base_RevC AEMv8-R (license-gated model).
    Fvp,
    /// PX4-SITL host (the uORB middleware). Issue 0341 — expressible so the
    /// uORB axis has a home; carried as a CarveOut (no CI runner builds SITL).
    Px4,
}

impl PlatformId {
    /// Stable index for the allocator formulas. Bounded — extending the
    /// enum extends the port/domain bands; the injectivity gate re-proves
    /// collision-freedom on every run.
    pub const fn index(self) -> u16 {
        match self {
            PlatformId::Native => 0,
            PlatformId::ZephyrNativeSim => 1,
            PlatformId::FreertosMps2 => 2,
            PlatformId::NuttxArm => 3,
            PlatformId::NuttxRiscv => 4,
            PlatformId::ThreadxLinux => 5,
            PlatformId::ThreadxRiscv64 => 6,
            PlatformId::Esp32Qemu => 7,
            PlatformId::QemuBaremetal => 8,
            PlatformId::Stm32F4 => 9,
            PlatformId::Fvp => 10,
            PlatformId::Px4 => 11,
        }
    }

    /// The `platform = "..."` token(s) `examples/fixtures.toml` spells this
    /// platform with — the SSoT for that vocabulary, in both directions
    /// ([`PlatformId::from_fixture_token`] is its inverse, gated by
    /// `fixture_token_mapping_round_trips`).
    ///
    /// It is one-to-MANY: `Esp32Qemu` covers both the RTOS lane (`esp32`) and the
    /// bare-metal one (`qemu-esp32-baremetal`). Selecting the platform must select
    /// both, so callers iterate the slice rather than taking a single token.
    ///
    /// One home on purpose. Before phase-318 W4.d this existed only as
    /// `platform_from_str` inside `tests/matrix_fixture_coverage.rs`, and the
    /// forward direction got hand-written a second time — with
    /// `qemu-esp32-baremetal` attributed to the wrong platform. A second spelling
    /// of a mapping is the recurring defect class in this repo (CLAUDE.md "add ONE
    /// shared helper rather than a second spelling").
    pub const fn fixture_tokens(self) -> &'static [&'static str] {
        match self {
            PlatformId::Native => &["native"],
            PlatformId::ZephyrNativeSim => &["zephyr"],
            PlatformId::FreertosMps2 => &["freertos"],
            PlatformId::NuttxArm => &["nuttx"],
            PlatformId::NuttxRiscv => &["nuttx-riscv"],
            PlatformId::ThreadxLinux => &["threadx-linux"],
            PlatformId::ThreadxRiscv64 => &["threadx-riscv64"],
            PlatformId::Esp32Qemu => &["esp32", "qemu-esp32-baremetal"],
            PlatformId::QemuBaremetal => &["qemu-arm-baremetal"],
            PlatformId::Stm32F4 => &["stm32f4"],
            PlatformId::Fvp => &["fvp"],
            // Carried as a CarveOut (no CI runner builds PX4-SITL), so no row
            // spells this today. The token is still declared: the vocabulary has
            // to name every platform, or a lane that later gains PX4 fixtures
            // selects nothing for them and looks fast rather than broken.
            PlatformId::Px4 => &["px4"],
        }
    }

    /// The `just` module that owns this platform's build/test verbs — the one a
    /// CI job runs as `just <module> …`.
    ///
    /// A THIRD vocabulary after `PlatformId` and the fixtures.toml tokens, so it
    /// lives here with the other two rather than being hand-listed in a workflow
    /// yml, where nothing would notice it going stale. `nightly.yml`'s platform
    /// list was hand-written exactly that way.
    ///
    /// Not injective: `NuttxArm` and `NuttxRiscv` share `nuttx` (which owns
    /// `build-riscv-*`), and `Fvp` is built by `just zephyr build-fvp-*`. Callers
    /// that need a job list must dedupe.
    pub const fn just_module(self) -> &'static str {
        match self {
            PlatformId::Native => "native",
            PlatformId::ZephyrNativeSim | PlatformId::Fvp => "zephyr",
            PlatformId::FreertosMps2 => "freertos",
            PlatformId::NuttxArm | PlatformId::NuttxRiscv => "nuttx",
            PlatformId::ThreadxLinux => "threadx_linux",
            PlatformId::ThreadxRiscv64 => "threadx_riscv64",
            PlatformId::Esp32Qemu => "esp32",
            PlatformId::QemuBaremetal => "qemu",
            PlatformId::Stm32F4 => "stm32f4",
            PlatformId::Px4 => "px4",
        }
    }

    /// `examples/fixtures.toml` `platform` string → matrix platform. Inverse of
    /// [`PlatformId::fixture_tokens`].
    pub fn from_fixture_token(s: &str) -> Option<PlatformId> {
        PlatformId::ALL
            .iter()
            .copied()
            .find(|p| p.fixture_tokens().contains(&s))
    }

    pub const ALL: &'static [PlatformId] = &[
        PlatformId::Native,
        PlatformId::ZephyrNativeSim,
        PlatformId::FreertosMps2,
        PlatformId::NuttxArm,
        PlatformId::NuttxRiscv,
        PlatformId::ThreadxLinux,
        PlatformId::ThreadxRiscv64,
        PlatformId::Esp32Qemu,
        PlatformId::QemuBaremetal,
        PlatformId::Stm32F4,
        PlatformId::Fvp,
        PlatformId::Px4,
    ];
}

/// RMW axis.
///
/// Issue 0341 — `Uorb` is declared supported in ARCHITECTURE §2
/// (`rmw-{zenoh,xrce,cyclonedds,uorb}`) with a real crate
/// (`packages/px4/nros-rmw-uorb`) and example (`packages/testing/nros-px4-register-check`), so it
/// must be *expressible* in the matrix. It carries a documented CarveOut cell
/// rather than a Runtime lane: uORB runs inside a PX4-SITL build that no CI
/// runner here provides. An expressible-but-carved-out axis is honest; an
/// inexpressible one hides the gap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rmw {
    Zenoh,
    Cyclonedds,
    Xrce,
    Uorb,
}

impl Rmw {
    pub const fn index(self) -> u16 {
        match self {
            Rmw::Zenoh => 0,
            Rmw::Cyclonedds => 1,
            Rmw::Xrce => 2,
            Rmw::Uorb => 3,
        }
    }
}

/// Language axis. `Mixed` exists only for `Kind::Workspace` cells.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    C,
    Cpp,
    Mixed,
}

impl Lang {
    /// Maps onto the historical [`TestLang`] port multiplier, extended
    /// with a fourth column for `Mixed` (the injectivity gate caught the
    /// original share-the-rust-slot idea colliding on platforms that run
    /// BOTH a rust and a mixed workspace cell — e.g. zephyr EntryPubsub).
    pub const fn port_index(self) -> u16 {
        match self {
            Lang::Rust => 0,
            Lang::C => 1,
            Lang::Cpp => 2,
            Lang::Mixed => 3,
        }
    }

    pub const fn as_test_lang(self) -> TestLang {
        match self {
            Lang::Rust | Lang::Mixed => TestLang::Rust,
            Lang::C => TestLang::C,
            Lang::Cpp => TestLang::Cpp,
        }
    }
}

/// Workload axis — each value is a stock-ROS-demo behavior contract the
/// shared checker knows how to assert (RFC-0051 §2).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Workload {
    Pubsub,
    Service,
    Action,
    /// Workspace Entry boot + pubsub delivery (the `zephyr_entry` class).
    EntryPubsub,
    CustomMsg,
    Logging,
    Qos,
    Params,
    Lifecycle,
    Safety,
    RealtimeTiers,
    Multihost,
    /// Launch/model `<remap>` + `~` private names reach the WIRE remapped
    /// (phase-306 W4, issue 0255).
    Remap,
}

impl Workload {
    /// Port-band offset. Pubsub/Service/Action keep the historical
    /// variant offsets (0/10/20); the workspace workloads take the
    /// 30..=110 band within each platform's lang column (stride 100 —
    /// bands never overlap the variant offsets).
    pub const fn port_offset(self) -> u16 {
        match self {
            Workload::Pubsub => 0,
            Workload::Service => 10,
            Workload::Action => 20,
            Workload::EntryPubsub => 30,
            Workload::CustomMsg => 40,
            Workload::Logging => 50,
            Workload::Qos => 60,
            Workload::Params => 70,
            Workload::Lifecycle => 80,
            Workload::Safety => 90,
            Workload::RealtimeTiers => 91,
            Workload::Multihost => 92,
            Workload::Remap => 93,
        }
    }

    /// Maps the three classic variants onto the historical enum (the
    /// QEMU harness APIs still take [`TestVariant`]).
    pub const fn as_test_variant(self) -> Option<TestVariant> {
        match self {
            Workload::Pubsub => Some(TestVariant::Pubsub),
            Workload::Service => Some(TestVariant::Service),
            Workload::Action => Some(TestVariant::Action),
            _ => None,
        }
    }
}

/// What the cell exercises.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Single-node example pair (talker/listener, server/client).
    Example,
    /// Entry-pkg workspace (`nros ws` shape, launch-driven).
    Workspace,
    /// nano-ros node against a REAL ROS 2 peer.
    Interop,
    /// Declarative bridge chains.
    Bridge,
}

/// Coverage tier — the load-bearing part of the table.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tier {
    /// A runtime e2e lane exists (or must exist — the consumer runs it).
    Runtime,
    /// Compiles/links as a build-stage fixture; no runtime lane yet. The
    /// string says what unlocks it.
    BuildOnly(&'static str),
    /// Deliberately unsupported / not applicable. The string is the
    /// recorded reason (audit E5).
    CarveOut(&'static str),
}

/// One cell of the matrix.
///
/// Issue 0327 — the ROS **edition** (ARCHITECTURE §2's third axis) is
/// deliberately NOT a `Cell` field: it is a PER-RUN GLOBAL, not a per-cell
/// dimension. A run targets one edition via `NROS_ROS_EDITION`
/// (`ros_env::test_edition()`, default `jazzy`) and executes the whole matrix
/// against it — there is no "jazzy pubsub" vs "humble pubsub" as distinct cells
/// in one run, so folding edition into `Cell` would multiply every row ×N
/// editions with no per-cell distinction. Edition coverage (which editions run,
/// and the humble/iron `rmw_zenoh_cpp` carve-out) is documented in
/// `examples/README.md`'s coverage matrix + ARCHITECTURE §2, not enumerated
/// here. (The prior silence about which kind of axis edition was is what let the
/// per-cell `ros_editions_*` files drift from RFC-0051.)
#[derive(Copy, Clone, Debug)]
pub struct Cell {
    pub platform: PlatformId,
    pub lang: Lang,
    pub rmw: Rmw,
    pub workload: Workload,
    pub kind: Kind,
    pub tier: Tier,
}

const fn cell(
    platform: PlatformId,
    lang: Lang,
    rmw: Rmw,
    workload: Workload,
    kind: Kind,
    tier: Tier,
) -> Cell {
    Cell {
        platform,
        lang,
        rmw,
        workload,
        kind,
        tier,
    }
}

/// A thing the test-tier tooling can treat as a matrix cell, whichever list it
/// came from (issue 0352 / phase-324). `matrix::CELLS` holds baked
/// self-contained cells; `crate::interop::CELLS` holds interop/bridge cells in
/// their richer shape. Both expose their underlying [`Cell`] so `ci_lane`
/// selection, coordinate derivation and coverage all iterate one type without
/// caring which list a cell belongs to.
pub trait TestCell {
    fn cell(&self) -> &Cell;
}

impl TestCell for Cell {
    fn cell(&self) -> &Cell {
        self
    }
}

// Shorthand used by the seed table below.
use Kind::*;
use Lang::*;
use PlatformId::*;
use Rmw::*;
use Tier::*;
use Workload::*;

/// W6 (2026-07-18) decided each cyclone/xrce-on-RTOS gap cell. Implement-
/// worthy cells (native rust cyclone service/action; threadx C cyclone
/// service/action; threadx C++ cyclone pubsub) are tracked in issue #233
/// and stay BuildOnly until wired; the rest are firm CarveOuts.
const CYCLONE_RUST_RTOS_CARVE: &str =
    "cyclone-on-RTOS is C/C++ only; pure-rust image has no cyclone backend symbol (#163 class)";
const XRCE_RTOS_CARVE: &str =
    "no XRCE agent-locator bake off Zephyr; rust-XRCE-on-bare-RTOS is not a shipped config";

/// The seed table (phase-295 W1): codifies the 2026-07-17 survey's REAL
/// coverage. Every pre-295 runtime lane appears as a `Runtime` cell;
/// every known gap as `BuildOnly`/`CarveOut` with its reason. The matrix
/// consumers (W3) iterate this; the fixture coverage gate cross-checks
/// it against `examples/fixtures.toml`.
#[rustfmt::skip]
pub const CELLS: &[Cell] = &[
    // ── Example kind: the classic pubsub/service/action pairs ──────────
    // Native (ephemeral isolation; all three RMWs have runtime lanes).
    cell(Native, Rust, Zenoh,      Pubsub,  Example, Runtime),
    cell(Native, C,    Zenoh,      Pubsub,  Example, Runtime),
    cell(Native, Cpp,  Zenoh,      Pubsub,  Example, Runtime),
    cell(Native, Rust, Zenoh,      Service, Example, Runtime),
    cell(Native, C,    Zenoh,      Service, Example, Runtime),
    cell(Native, Cpp,  Zenoh,      Service, Example, Runtime),
    cell(Native, Rust, Zenoh,      Action,  Example, Runtime),
    cell(Native, C,    Zenoh,      Action,  Example, Runtime),
    cell(Native, Cpp,  Zenoh,      Action,  Example, Runtime),
    cell(Native, Rust, Cyclonedds, Pubsub,  Example, Runtime),
    cell(Native, C,    Cyclonedds, Pubsub,  Example, Runtime),
    cell(Native, Cpp,  Cyclonedds, Pubsub,  Example, Runtime),
    cell(Native, C,    Cyclonedds, Service, Example, Runtime),
    cell(Native, Cpp,  Cyclonedds, Service, Example, Runtime),
    // issue #233 cell 1 — proven: rust cyclone service pair delivers
    // (test_native_cyclonedds_rust_service).
    cell(Native, Rust, Cyclonedds, Service, Example, Runtime),
    cell(Native, C,    Cyclonedds, Action,  Example, Runtime),
    cell(Native, Cpp,  Cyclonedds, Action,  Example, Runtime),
    // issue #234 — RESOLVED: rust cyclone action pair delivers the order-10
    // Fibonacci result (test_native_cyclonedds_rust_action). The action's
    // `register_protocol_types` now registers the `action_msgs` descriptors
    // through the generic `nros_rmw::register_type_descriptor` seam instead of
    // the cfg-gated named-backend call that compiled out of the example build.
    cell(Native, Rust, Cyclonedds, Action,  Example, Runtime),
    cell(Native, C,    Xrce,       Pubsub,  Example, Runtime),
    cell(Native, Rust, Xrce,       Pubsub,  Example, Runtime),
    cell(Native, Cpp,  Xrce,       Pubsub,  Example, Runtime),
    cell(Native, C,    Xrce,       Service, Example, Runtime),
    cell(Native, Rust, Xrce,       Service, Example, Runtime),
    cell(Native, Cpp,  Xrce,       Service, Example, Runtime),
    cell(Native, C,    Xrce,       Action,  Example, Runtime),
    cell(Native, Rust, Xrce,       Action,  Example, Runtime),
    cell(Native, Cpp,  Xrce,       Action,  Example, Runtime),

    // Zephyr native_sim — zenoh + cyclone + xrce, all three langs
    // (the zephyr.rs families; W4 bakes: cyclone domains 22–30, xrce
    // agents 2400+ — `alloc::{domain_of,xrce_agent_port_of}`).
    cell(ZephyrNativeSim, Rust, Zenoh,      Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Zenoh,      Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Zenoh,      Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh,      Service, Example, Runtime),
    cell(ZephyrNativeSim, C,    Zenoh,      Service, Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Zenoh,      Service, Example, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh,      Action,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Zenoh,      Action,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Zenoh,      Action,  Example, Runtime),
    cell(ZephyrNativeSim, Rust, Cyclonedds, Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Cyclonedds, Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Cyclonedds, Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Rust, Cyclonedds, Service, Example, Runtime),
    cell(ZephyrNativeSim, C,    Cyclonedds, Service, Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Cyclonedds, Service, Example, Runtime),
    cell(ZephyrNativeSim, Rust, Cyclonedds, Action,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Cyclonedds, Action,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Cyclonedds, Action,  Example, Runtime),
    cell(ZephyrNativeSim, Rust, Xrce,       Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Xrce,       Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Xrce,       Pubsub,  Example, Runtime),
    cell(ZephyrNativeSim, Rust, Xrce,       Service, Example, Runtime),
    cell(ZephyrNativeSim, C,    Xrce,       Service, Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Xrce,       Service, Example, Runtime),
    cell(ZephyrNativeSim, Rust, Xrce,       Action,  Example, Runtime),
    cell(ZephyrNativeSim, C,    Xrce,       Action,  Example, Runtime),
    cell(ZephyrNativeSim, Cpp,  Xrce,       Action,  Example, Runtime),

    // FreeRTOS / NuttX-arm / ThreadX-linux — the rtos_e2e 3×3 zenoh block.
    cell(FreertosMps2, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(FreertosMps2, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(FreertosMps2, Rust, Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, C,    Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, Rust, Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, C,    Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, Rust, Cyclonedds, Pubsub, Example,
         BuildOnly("fixture retired in phase-220.C (cmake-bridge removed); \
                    freertos_qemu.rs lanes #[ignore]d pending the 214.S.5.b \
                    pure-cargo BSP gate — issue #233 tracks restore-vs-carve")),
    cell(FreertosMps2, Rust, Xrce,       Pubsub, Example, CarveOut(XRCE_RTOS_CARVE)),

    cell(NuttxArm, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(NuttxArm, Rust, Zenoh, Service, Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Service, Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Service, Example, Runtime),
    cell(NuttxArm, Rust, Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, Rust, Cyclonedds, Pubsub, Example, CarveOut(CYCLONE_RUST_RTOS_CARVE)),
    cell(NuttxArm, Rust, Xrce,       Pubsub, Example, CarveOut(XRCE_RTOS_CARVE)),

    cell(ThreadxLinux, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, Rust, Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, Rust, Zenoh, Action,  Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Action,  Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Action,  Example, Runtime),
    // threadx-linux cyclone: C pubsub pair proven (native_api #215 lane);
    // service/action fixtures build but have no runtime lane.
    cell(ThreadxLinux, C,   Cyclonedds, Pubsub,  Example, Runtime),
    // issue #233 cell 3 — threadx C cyclone service proven (test_threadx_linux_cyclonedds_service).
    cell(ThreadxLinux, C,   Cyclonedds, Service, Example, Runtime),
    // issue #233 cell 3 — threadx C cyclone action proven (test_threadx_linux_cyclonedds_action).
    cell(ThreadxLinux, C,   Cyclonedds, Action,  Example, Runtime),
    // issue #233 cell 4 — threadx C++ cyclone pubsub proven (test_threadx_linux_cyclonedds_cpp_talker_to_native_listener).
    cell(ThreadxLinux, Cpp, Cyclonedds, Pubsub,  Example, Runtime),

    // ThreadX riscv64 — pubsub + service runtime (all three langs; rtos_e2e
    // runs the full lang fan-out). Action examples + builders EXIST in all
    // three langs but were deliberately dropped from the run matrix in
    // phase-182.5 (action is the wall-clock critical path — see rtos_e2e.rs);
    // cyclone two-QEMU pubsub pairs proven (#214).
    cell(ThreadxRiscv64, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, Rust, Zenoh, Service, Example, Runtime),
    cell(ThreadxRiscv64, C,    Zenoh, Service, Example, Runtime),
    cell(ThreadxRiscv64, Cpp,  Zenoh, Service, Example, Runtime),
    cell(ThreadxRiscv64, Rust, Zenoh, Action, Example,
         BuildOnly("dropped from the action run matrix in 182.5 (wall-clock); examples + rtos_e2e builders exist")),
    cell(ThreadxRiscv64, C,    Zenoh, Action, Example,
         BuildOnly("dropped from the action run matrix in 182.5 (wall-clock); examples + rtos_e2e builders exist")),
    cell(ThreadxRiscv64, Cpp,  Zenoh, Action, Example,
         BuildOnly("dropped from the action run matrix in 182.5 (wall-clock); examples + rtos_e2e builders exist")),
    cell(ThreadxRiscv64, C,    Cyclonedds, Pubsub, Example, Runtime),
    cell(ThreadxRiscv64, Rust, Cyclonedds, Pubsub, Example, Runtime),
    // issue #235 — the cpp cyclone riscv64 fixtures existed (distinct
    // identity per node); the two-QEMU lane
    // (test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub) now consumes them.
    cell(ThreadxRiscv64, Cpp,  Cyclonedds, Pubsub, Example, Runtime),

    // NuttX riscv — the C talker example has a runtime lane
    // (c_riscv_nuttx_e2e); rust/cpp have NO standalone pubsub examples —
    // their riscv coverage is the realtime-tiers WORKSPACE lanes (rows
    // below), so don't claim Example-Runtime here.
    cell(NuttxRiscv, C,    Zenoh, Pubsub, Example, Runtime),
    cell(NuttxRiscv, Cpp,  Zenoh, Pubsub, Example,
         CarveOut("no standalone cpp pubsub example on rv-virt; runtime coverage rides the realtime-tiers workspace lane")),
    cell(NuttxRiscv, Rust, Zenoh, Pubsub, Example,
         CarveOut("no standalone rust pubsub example on rv-virt; runtime coverage rides the realtime-tiers workspace lane")),

    // ESP32 — rust pubsub runtime under the Espressif QEMU fork (plus the
    // workspace-entry lane in esp32_emulator.rs); service/action examples
    // are NOT authored (example set is talker/listener only). C/C++
    // build-only.
    cell(Esp32Qemu, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(Esp32Qemu, Rust, Zenoh, Service, Example,
         CarveOut("service/action examples not authored on esp32-qemu (talker/listener set only)")),
    cell(Esp32Qemu, Rust, Zenoh, Action,  Example,
         CarveOut("service/action examples not authored on esp32-qemu (talker/listener set only)")),
    cell(Esp32Qemu, C,    Zenoh, Pubsub,  Example,
         BuildOnly("IDF C runtime lane pending (espressif qemu fork drives rust only today)")),
    cell(Esp32Qemu, Cpp,  Zenoh, Pubsub,  Example,
         BuildOnly("IDF C++ runtime lane pending")),

    // Bare-metal RTIC (QEMU MPS2) — pubsub-only demo set by design.
    cell(QemuBaremetal, Rust, Zenoh, Pubsub, Example, Runtime),
    cell(QemuBaremetal, Rust, Zenoh, Service, Example,
         CarveOut("rtic demo set is pubsub-only by design (phase-289 scope)")),
    cell(QemuBaremetal, Rust, Zenoh, Action, Example,
         CarveOut("rtic demo set is pubsub-only by design (phase-289 scope)")),

    // STM32F4 hardware — build-only (#221: QEMU has no F4 ethernet model;
    // runtime proof rides the shared entry scaffold's QEMU RTIC lanes).
    cell(Stm32F4, Rust, Zenoh, Pubsub,  Example,
         BuildOnly("hardware-gated (#221); QEMU RTIC lanes are the runtime proof for the shared scaffold")),
    cell(Stm32F4, Rust, Zenoh, Service, Example,
         BuildOnly("hardware-gated (#221)")),
    cell(Stm32F4, Rust, Zenoh, Action,  Example,
         BuildOnly("hardware-gated (#221)")),

    // FVP — cyclone runtime (license-gated at run time), cpp + rust.
    // Issue 0232 / phase-320 W1.a — these were `Runtime`, which the lane can
    // never satisfy: the Base_RevC AEMv8-R model is license-walled
    // (`[gated.arm-fvp]` in nros-sdk-index.toml, user-supplied via ARM_FVP_DIR),
    // so `fvp_smoke` / `fvp_runtime_ws` skip on EVERY CI and dev host. Claiming
    // Runtime here is the exact shape of 0232's false green — a lane that always
    // skipped, so four walls "shipped invisible and were found by the ASI
    // consumer". A gap reads as a gap; an overclaim reads as confidence.
    //
    // The maintainer-run runtime gate still exists and still matters
    // (`just zephyr verify-fvp-runtime`); it is simply not coverage this matrix
    // can promise. Note the runtime-verified FVP artifact is the two-tier
    // workspace Entry, not these example cells.
    cell(Fvp, Cpp,  Cyclonedds, Pubsub, Example,
         BuildOnly("license-gated model; runtime needs ARM_FVP_DIR and is maintainer-run \
                    via `just zephyr verify-fvp-runtime` (phase-298)")),
    cell(Fvp, Rust, Cyclonedds, Pubsub, Example,
         BuildOnly("license-gated model; runtime needs ARM_FVP_DIR and is maintainer-run \
                    via `just zephyr verify-fvp-runtime` (phase-298)")),
    cell(Fvp, Cpp,  Zenoh,      Pubsub, Example,
         CarveOut("zenoh-pico needs POSIX API the FVP board conf doesn't enable (#217)")),

    // ── Workspace kind (Entry-pkg lanes; native-heavy today) ──────────
    cell(Native, Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(Native, C,     Zenoh, EntryPubsub, Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, EntryPubsub, Workspace, Runtime),
    cell(Native, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, C,     Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, Cpp,   Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, C,    Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Rust, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(NuttxArm, C,    Zenoh, EntryPubsub, Workspace, Runtime),
    // Corrected during the phase-295 W3.b entry consolidation: the seed
    // table marked the nuttx-arm C++ and all three nuttx-riscv EntryPubsub
    // rows `Runtime`, but no EntryPubsub fixture or lane exists at those
    // coordinates — the only nuttx workspace rows besides the C arm entry
    // are the REALTIME-TIERS entries (the fixtures.toml realtime rows +
    // workspace-rust-nuttx-riscv-realtime), which satisfied
    // the (platform, lang) coverage gate and masked the gap. The riscv C
    // runtime proof that exists is the STANDALONE talker example
    // (c_riscv_nuttx_e2e — the `(NuttxRiscv, C, Pubsub, Example)` cell).
    cell(NuttxArm, Cpp,  Zenoh, EntryPubsub, Workspace,
         BuildOnly("no nuttx-arm C++ EntryPubsub fixture/lane; only the RT-tiers C++ \
                    workspace builds at this coordinate — phase-295 W3.b finding, W6 wires it")),
    cell(NuttxRiscv, C,   Zenoh, EntryPubsub, Workspace,
         BuildOnly("no nuttx-riscv C EntryPubsub workspace fixture/lane (RT-tiers only; \
                    the standalone talker example is the riscv C runtime proof) — \
                    phase-295 W3.b finding, W6 wires it")),
    cell(NuttxRiscv, Cpp, Zenoh, EntryPubsub, Workspace,
         BuildOnly("no nuttx-riscv C++ EntryPubsub workspace fixture/lane (RT-tiers only) \
                    — phase-295 W3.b finding, W6 wires it")),
    cell(ThreadxLinux, Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ThreadxLinux, C,     Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ThreadxLinux, Cpp,   Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ThreadxLinux, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(NuttxArm,     Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    // See the nuttx-riscv correction above — the rust riscv workspace row
    // is realtime-only too (workspace-rust-nuttx-riscv-realtime); no
    // EntryPubsub image or lane exists. phase-295 W3.b finding.
    cell(NuttxRiscv,   Rust,  Zenoh, EntryPubsub, Workspace,
         BuildOnly("no nuttx-riscv rust EntryPubsub workspace fixture/lane (RT-tiers \
                    only) — phase-295 W3.b finding, W6 wires it")),
    cell(Esp32Qemu,    Rust, Zenoh, EntryPubsub, Workspace, Runtime),

    // Workspace feature workloads (native + zephyr today; per-lang rows
    // mirror the ws-* families).
    cell(Native, C,     Zenoh, CustomMsg, Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, CustomMsg, Workspace, Runtime),
    // Corrected during the phase-295 W3.b consolidation: the seed table
    // marked native rust CustomMsg/Qos `Runtime`, but no fixtures.toml row
    // builds `ws-{custom-msg,qos}-rust`'s `native_entry` and no test
    // consumes it (the C files' "C projection of the Rust demo" prose
    // described the WORKSPACE, not a lane; ws-qos-rust's only runtime lane
    // is the zephyr image). Single-entry natives also hit issue 0096
    // (in-process pub→sub never delivers), so wiring them needs split
    // talker/listener entries first — issue #233.
    cell(Native, Rust,  Zenoh, CustomMsg, Workspace,
         BuildOnly("ws-custom-msg-rust native_entry has no fixture row or runtime lane \
                    (needs an 0096 two-entry split) — phase-295 W3.b finding, W6 wires it")),
    cell(Native, Mixed, Zenoh, CustomMsg, Workspace, Runtime),
    cell(Native, C,     Zenoh, Logging,   Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Logging,   Workspace, Runtime),
    // Added during the phase-295 W3.b consolidation: the rust + mixed
    // logging lanes existed (tests/{,mixed_}logging_workspace_e2e.rs,
    // phase-263 A5) but the seed table never modeled them.
    cell(Native, Rust,  Zenoh, Logging,   Workspace, Runtime),
    cell(Native, Mixed, Zenoh, Logging,   Workspace, Runtime),
    cell(Native, C,     Zenoh, Qos,       Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Qos,       Workspace, Runtime),
    // See the CustomMsg rust row above — same phase-295 W3.b correction.
    cell(Native, Rust,  Zenoh, Qos,       Workspace,
         BuildOnly("ws-qos-rust native_entry has no fixture row or runtime lane (only \
                    the zephyr image is consumed) — phase-295 W3.b finding, W6 wires it")),
    cell(Native, Mixed, Zenoh, Qos,       Workspace, Runtime),
    cell(Native, C,     Zenoh, Params,    Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Params,    Workspace, Runtime),
    cell(Native, Rust,  Zenoh, Params,    Workspace, Runtime),
    cell(Native, C,     Zenoh, Lifecycle, Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Lifecycle, Workspace, Runtime),
    cell(Native, Rust,  Zenoh, Lifecycle, Workspace, Runtime),
    cell(Native, C,     Zenoh, Safety,    Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Safety,    Workspace, Runtime),
    cell(Native, Rust,  Zenoh, Safety,    Workspace, Runtime),
    // phase-306 W4 (issue 0255) — launch/model remap + `~` private name hits
    // the wire remapped. Rust only: the C/C++ `nros_cpp_declare_remap` path is
    // emitter-unit-tested (W3); a runtime C/C++ cell is residual.
    cell(Native, Rust,  Zenoh, Remap,     Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Params,    Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Lifecycle, Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Qos,       Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Safety,    Workspace, Runtime),

    // Realtime tiers + multihost.
    cell(Native, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Native, C,    Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Native, Cpp,  Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(ZephyrNativeSim, C,    Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(ZephyrNativeSim, Cpp,  Zenoh, RealtimeTiers, Workspace, Runtime),
    // Corrected during the phase-295 W4 re-bake: the realtime_tiers_e2e
    // consumer has ALWAYS run nuttx-arm {c,rust}, nuttx-riscv {rust,c} and
    // freertos c cells (fixtures.toml rows existed for each), but the seed
    // table only modeled the cpp rows — the (platform, lang) coverage gate
    // was satisfied by other workspace rows and masked the gap. Modeled so
    // the allocator derives every baked realtime port.
    cell(NuttxArm,   Cpp,  Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(NuttxArm,   C,    Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(NuttxArm,   Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(NuttxRiscv, Cpp,  Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(NuttxRiscv, C,    Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(NuttxRiscv, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(FreertosMps2, Cpp, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(FreertosMps2, C,   Zenoh, RealtimeTiers, Workspace, Runtime),
    // phase-297 W5 (RFC-0053) — ThreadX multi-tier run_tiers acceptance:
    // hosted simulation (pthread-backed ThreadX, host binary + NSOS host
    // sockets), port 9091 = port_of(ThreadxLinux, Rust, RealtimeTiers).
    cell(ThreadxLinux, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Native, Rust,  Zenoh, Multihost, Workspace, Runtime),
    cell(Native, C,     Zenoh, Multihost, Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Multihost, Workspace, Runtime),
    cell(Native, Mixed, Zenoh, Multihost, Workspace, Runtime),
    // The embedded multihost lane is the RUST robot1 zephyr image (276 W6);
    // corrected from Cpp during the phase-295 W3.b consolidation.
    cell(ZephyrNativeSim, Rust, Zenoh, Multihost, Workspace, Runtime),

    // Cross-process service/action roundtrips (phase-263 A1/A4; issue 0096
    // forces the two-process topology) — tests/roundtrip_xprocess_e2e.rs.
    cell(Native, Rust,  Zenoh, Service, Workspace, Runtime),
    cell(Native, C,     Zenoh, Service, Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Service, Workspace, Runtime),
    cell(Native, Mixed, Zenoh, Service, Workspace, Runtime),
    cell(Native, Rust,  Zenoh, Action,  Workspace, Runtime),
    cell(Native, C,     Zenoh, Action,  Workspace, Runtime),
    cell(Native, Cpp,   Zenoh, Action,  Workspace, Runtime),
    cell(Native, Mixed, Zenoh, Action,  Workspace, Runtime),

    // Workspace RMW variants (thin today: 80/82 rows are zenoh — issue #233).
    cell(Native, Rust, Cyclonedds, EntryPubsub, Workspace, Runtime),
    cell(Native, Rust, Xrce,       EntryPubsub, Workspace, Runtime),

    // ── Interop & Bridge kinds — issue 0352 / phase-324 ────────────────
    // These cells carry a nano side that is BUILT plus an ephemeral PEER and a
    // DIRECTION, a shape `Cell` cannot express. They live in `crate::interop`
    // (`interop::CELLS`) in that formulation, joined to their build + test
    // recipe by a `Binding` and gated by `tests/matrix_fixture_coverage.rs`.
    // `matrix::CELLS` is baked/self-contained only.

    // ── uORB (PX4-SITL) — issue 0341 ───────────────────────────────────
    // uORB is a declared RMW (ARCHITECTURE §2) with a real crate
    // (nros-rmw-uorb) + example (packages/testing/nros-px4-register-check), but its runtime lane
    // is a PX4-SITL build no CI runner here provides. Expressible + carved out,
    // so the gap is visible rather than inexpressible.
    cell(Px4, Cpp, Uorb, Pubsub, Example,
         CarveOut("uORB runs only inside a PX4-SITL build (just px4 …); no CI runner \
                   builds SITL. `packages/testing/nros-px4-register-check` is the source of truth.")),
];

/// Runtime cells only — what the matrix consumers iterate.
pub fn runtime_cells() -> impl Iterator<Item = &'static Cell> {
    CELLS.iter().filter(|c| matches!(c.tier, Tier::Runtime))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Mixed` is a workspace-only axis value.
    #[test]
    fn mixed_lang_only_in_workspace_cells() {
        for c in CELLS {
            if matches!(c.lang, Lang::Mixed) {
                assert!(
                    matches!(c.kind, Kind::Workspace),
                    "Mixed cell outside Workspace: {c:?}"
                );
            }
        }
    }

    /// No duplicate coordinates — each (platform, lang, rmw, workload,
    /// kind) appears at most once.
    #[test]
    fn cells_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in CELLS {
            let key = (
                c.platform.index(),
                c.lang as u8 as u16,
                c.rmw.index(),
                c.workload.port_offset(),
                c.kind as u8,
            );
            assert!(seen.insert(key), "duplicate cell: {c:?}");
        }
    }

    /// Every carve-out / build-only reason is non-empty (audit E5).
    #[test]
    fn gap_tiers_carry_reasons() {
        for c in CELLS {
            match c.tier {
                Tier::BuildOnly(r) | Tier::CarveOut(r) => {
                    assert!(!r.is_empty(), "empty reason: {c:?}")
                }
                Tier::Runtime => {}
            }
        }
    }
}

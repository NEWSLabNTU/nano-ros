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
        }
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
    ];
}

/// RMW axis.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rmw {
    Zenoh,
    Cyclonedds,
    Xrce,
}

impl Rmw {
    pub const fn index(self) -> u16 {
        match self {
            Rmw::Zenoh => 0,
            Rmw::Cyclonedds => 1,
            Rmw::Xrce => 2,
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
    // issue #233 — rust cyclone action fails at CREATION
    // (ActionCreationFailed): the typed-action-descriptor path C/C++'s
    // descriptors.cpp fills, the pure-rust path does not. Not a fixture gap.
    cell(Native, Rust, Cyclonedds, Action,  Example,
         BuildOnly("rust cyclone action = ActionCreationFailed (typed-descriptor gap) — issue #234")),
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

    // ThreadX riscv64 — pubsub + service runtime; action examples absent;
    // cyclone two-QEMU pubsub pairs proven (#214).
    cell(ThreadxRiscv64, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxRiscv64, Rust, Zenoh, Service, Example, Runtime),
    cell(ThreadxRiscv64, C,    Zenoh, Service, Example, Runtime),
    cell(ThreadxRiscv64, Cpp,  Zenoh, Service, Example,
         CarveOut("cpp service/action examples not implemented; port slots reserved (platform.rs)")),
    cell(ThreadxRiscv64, Rust, Zenoh, Action, Example,
         CarveOut("action examples not implemented on threadx-riscv64 (example set is pubsub+service); reserved slots in platform.rs")),
    cell(ThreadxRiscv64, C,    Cyclonedds, Pubsub, Example, Runtime),
    cell(ThreadxRiscv64, Rust, Cyclonedds, Pubsub, Example, Runtime),
    cell(ThreadxRiscv64, Cpp,  Cyclonedds, Pubsub, Example,
         BuildOnly("no cpp cyclone riscv64 fixture yet — needs build variant + QEMU pair, issue #235")),

    // NuttX riscv — C/C++/rust runtime lanes (phase #199 follow-up).
    cell(NuttxRiscv, C,    Zenoh, Pubsub, Example, Runtime),
    cell(NuttxRiscv, Cpp,  Zenoh, Pubsub, Example, Runtime),
    cell(NuttxRiscv, Rust, Zenoh, Pubsub, Example, Runtime),

    // ESP32 — rust runtime under the Espressif QEMU fork; C/C++ build-only.
    cell(Esp32Qemu, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(Esp32Qemu, Rust, Zenoh, Service, Example, Runtime),
    cell(Esp32Qemu, Rust, Zenoh, Action,  Example, Runtime),
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
    cell(Fvp, Cpp,  Cyclonedds, Pubsub, Example, Runtime),
    cell(Fvp, Rust, Cyclonedds, Pubsub, Example, Runtime),
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

    // ── Interop kind (nano ↔ real ROS 2; reduced workload set) ────────
    cell(Native, Rust, Zenoh,      Pubsub,  Interop, Runtime),
    cell(Native, Rust, Zenoh,      Service, Interop, Runtime),
    cell(Native, Rust, Cyclonedds, Pubsub,  Interop, Runtime),
    cell(Native, Rust, Cyclonedds, Service, Interop, Runtime),
    cell(Native, Rust, Xrce,       Pubsub,  Interop, Runtime),
    cell(Native, Rust, Xrce,       Service, Interop, Runtime),
    cell(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop, Runtime),
    cell(Native, Rust, Zenoh, Lifecycle, Interop, Runtime),

    // ── Bridge kind ────────────────────────────────────────────────────
    cell(Native, Rust, Zenoh, Pubsub, Bridge, Runtime), // zenoh→cyclonedds
    cell(Native, Rust, Xrce,  Pubsub, Bridge, Runtime), // zenoh→xrce
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

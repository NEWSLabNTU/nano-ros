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
    /// The Linux host board over the `posix` platform. Isolation is EPHEMERAL
    /// (ports/domains picked at runtime) — the allocator's baked formula does
    /// not apply.
    ///
    /// phase-337 W8.b renamed this from `Native` (RFC-0064 R3): "native"
    /// implies any general-purpose OS, and Windows/macOS do not build. The
    /// PLATFORM is still `posix` — only the board-level promise is narrowed to
    /// what `just ci` actually exercises.
    Linux,
    /// Zephyr native_sim (NSOS host sockets).
    ZephyrNativeSim,
    /// Zephyr on QEMU MPS2-AN385 (Cortex-M3) — Zephyr's OWN IP stack over the
    /// `eth_smsc911x` driver, 32-bit.
    ///
    /// phase-337 W2 added this because "Zephyr" previously meant exactly one
    /// config: `native_sim/native/64`, where sockets are OFFLOADED to the host
    /// and the pointer width is the host's. That is a board, not a platform —
    /// and the difference is not academic. Bringing this witness up cost five
    /// real defects (a 32-bit `size_t`/`uintptr_t` header conflict, an atomics
    /// feature gated on an arch list, a staticlib with no allocator or panic
    /// handler off native_sim, a duplicated cmake feature string, and a board
    /// with no entropy device), every one of them invisible to native_sim.
    ///
    /// This used to read "cells here are C/C++ only: the pinned
    /// `zephyr-lang-rust` cannot compile for any board whose devicetree has gpio
    /// nodes (issue 0432)". **0432 was RESOLVED 2026-08-12 by phase-346 W2/W3**;
    /// the Rust leaf has built since and `zephyr_cortex_m_rust_zenoh_pubsub_e2e`
    /// runs it. Corrected phase-441 W1, which read the sentence as a constraint
    /// and had to measure it to find out otherwise. All three languages are
    /// live here.
    ZephyrQemuCortexM,
    /// FreeRTOS on QEMU MPS2-AN385 (lwIP).
    FreertosMps2,
    /// FreeRTOS POSIX simulator on the build host (phase-370).
    ///
    /// The kernel's `ThirdParty/GCC/Posix` port: FreeRTOS tasks are pthreads,
    /// the tick is a host timer signal, and the network stack is the HOST's
    /// rather than lwIP. That last one is why it is a separate witness from
    /// `FreertosMps2` and not a config of it — the RMW below it is the `posix`
    /// branch's host ddsrt, so the two exercise different halves of the family.
    ///
    /// Cells here are C/C++ only: the POSIX port has no Rust entry shape of its
    /// own, and this board carries no Rust board crate.
    FreertosPosix,
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
            PlatformId::Linux => 0,
            PlatformId::ZephyrNativeSim => 1,
            PlatformId::FreertosMps2 => 2,
            PlatformId::NuttxArm => 3,
            PlatformId::NuttxRiscv => 4,
            PlatformId::ThreadxLinux => 5,
            PlatformId::ThreadxRiscv64 => 6,
            PlatformId::Esp32Qemu => 7,
            PlatformId::QemuBaremetal => 8,
            // 9 was `Stm32F4` until phase-337 W7.a. Renumbering `Fvp`/`Px4`
            // down into the gap is free: both carry ZERO Runtime cells, so no
            // fixture image, locator or domain was ever baked from their band.
            //
            // phase-337 W2.c takes 9 for the same reason, and moves them again.
            // The index is NOT free real estate: `alloc::domain_of` gives every
            // platform a 21-wide window out of the 232 DDS domains, which fits
            // exactly 11 — so a twelfth platform at index 11 computes domain 233
            // and `domains_valid` rejects it. The scarce resource is therefore a
            // LOW index, and it belongs to platforms that actually bake. `Fvp`
            // and `Px4` still bake nothing (tier 3 / CarveOut), so their windows
            // remain unreachable arithmetic wherever they sit; a witness with
            // Runtime cells cannot say the same. If either ever gains a Runtime
            // cell, `domains_valid` fails immediately — which is the right
            // answer, because at that point the window scheme is genuinely full
            // and needs narrowing, not another renumber.
            PlatformId::ZephyrQemuCortexM => 9,
            // phase-370 takes 10 on the rule the paragraph above states: a LOW
            // index is the scarce resource and belongs to a platform that
            // BAKES. `domain_of` is `1 + index*21 + slot*3 + lang`, so index 10
            // tops out at 231 and index 11 at 252 — 10 is the last usable slot,
            // and it was held by a tier-3 witness with zero Runtime cells.
            // `Fvp` and `Px4` move up into arithmetic that is unreachable for
            // exactly as long as they bake nothing, which is the same trade the
            // comment above already made on their behalf.
            PlatformId::FreertosPosix => 10,
            PlatformId::Fvp => 11,
            PlatformId::Px4 => 12,
        }
    }

    /// The `platform = "..."` token(s) `examples/fixtures.toml` spells this
    /// platform with — the SSoT for that vocabulary, in both directions
    /// ([`PlatformId::from_fixture_token`] is its inverse, gated by
    /// `fixture_token_mapping_round_trips`).
    ///
    /// It is one-to-MANY *in shape*, and today every platform spells exactly one
    /// token. `Esp32Qemu` was the sole exception — it carried `esp32` and
    /// `qemu-esp32-baremetal` for one platform, with no stated reason — and
    /// phase-437 W4a collapsed that under RFC-0093 R6 (a fixture platform names
    /// a FAMILY, never a board). The slice stays because callers must not
    /// assume a single token: a platform that legitimately needs two, the way
    /// `nuttx`/`nuttx-riscv` would if they were one `PlatformId`, must not
    /// require touching every call site.
    ///
    /// One home on purpose. Before phase-318 W4.d this existed only as
    /// `platform_from_str` inside `tests/matrix_fixture_coverage.rs`, and the
    /// forward direction got hand-written a second time — with the esp32
    /// bare-metal token attributed to the wrong platform. A second spelling
    /// of a mapping is the recurring defect class in this repo (CLAUDE.md "add ONE
    /// shared helper rather than a second spelling").
    pub const fn fixture_tokens(self) -> &'static [&'static str] {
        match self {
            // phase-337 W8.c — moved from `native` WITH the two other
            // token-derived vocabularies, because the token is not only a
            // token: it is also the first argument of
            // `scripts/build/fixtures-build.sh` / `workspace-fixtures-build.sh`,
            // the scope name in `fixture-make-driver.sh`, and the coordinate
            // prefix `fixture-lane.sh` greps. Renaming any one of those alone
            // would leave rows saying `linux` and the builder that produces them
            // still called with `native` — two spellings of one fact.
            //
            // The LANE (`lane=linux`, `_NROS_LANES`) and the `just` MODULE
            // (`just linux …`, see `just_module` below) followed in phase-500
            // W2. The `examples/linux/` DIRECTORY is a `dir =` value, not
            // this token; it moves in phase-500 W4.
            PlatformId::Linux => &["linux"],
            PlatformId::ZephyrNativeSim => &["zephyr"],
            // phase-337 W2.c/W2.e declared this token with NO `fixtures.toml`
            // row spelling it, because the board was built by the west leaves
            // lane off its own bash matrix. phase-350 W1 gave that lane rows, so
            // three `builder = "west"` rows spell it now (the mps2 talker
            // witness, one per language).
            //
            // Those rows are why the token is not just cosmetic: the witness
            // leaves spell the SAME `dir`, `lang` and `rmw` as their native_sim
            // siblings, so putting both boards under `zephyr` would give two
            // distinct rows one coordinate — and a coordinate is what every
            // lane-scoped build and the whole staleness gate select on.
            PlatformId::ZephyrQemuCortexM => &["zephyr-cortex-m"],
            PlatformId::FreertosMps2 => &["freertos"],
            // A token of its own, for the reason the ZephyrQemuCortexM comment
            // above gives: the witness leaves spell the same `lang` and `rmw`
            // as their mps2 siblings, so one shared token would give two
            // distinct rows one coordinate — and a coordinate is what every
            // lane-scoped build and the staleness gate select on.
            PlatformId::FreertosPosix => &["freertos-posix"],
            PlatformId::NuttxArm => &["nuttx"],
            PlatformId::NuttxRiscv => &["nuttx-riscv"],
            PlatformId::ThreadxLinux => &["threadx-linux"],
            PlatformId::ThreadxRiscv64 => &["threadx-riscv64"],
            PlatformId::Esp32Qemu => &["esp32"],
            // RFC-0093 R6 — the FAMILY, not the board. The board is
            // `mps2-an385-baremetal`; `baremetal` is free because this is the
            // only bare-metal family.
            PlatformId::QemuBaremetal => &["baremetal"],
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
            // phase-500 W2 — the module was `native` until the maintainer's
            // 2026-10-14 ruling (native = any general-purpose host; the host
            // build is Linux-only). The old spelling survives one cycle as a
            // deprecated root forwarder; this answer is the real name.
            PlatformId::Linux => "linux",
            // All three are `just zephyr …` — one module, three boards. That is
            // the RFC-0064 shape working as intended: a board is a conf bundle
            // under a family's recipes, not a namespace of its own.
            PlatformId::ZephyrNativeSim | PlatformId::Fvp | PlatformId::ZephyrQemuCortexM => {
                "zephyr"
            }
            // One module, two boards — the RFC-0064 shape, same as the three
            // Zephyr witnesses above. `just freertos build-posix-*` owns it.
            PlatformId::FreertosMps2 | PlatformId::FreertosPosix => "freertos",
            PlatformId::NuttxArm | PlatformId::NuttxRiscv => "nuttx",
            PlatformId::ThreadxLinux => "threadx_linux",
            PlatformId::ThreadxRiscv64 => "threadx_riscv64",
            PlatformId::Esp32Qemu => "esp32",
            PlatformId::QemuBaremetal => "qemu",
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
        PlatformId::Linux,
        PlatformId::ZephyrNativeSim,
        PlatformId::ZephyrQemuCortexM,
        PlatformId::FreertosMps2,
        PlatformId::FreertosPosix,
        PlatformId::NuttxArm,
        PlatformId::NuttxRiscv,
        PlatformId::ThreadxLinux,
        PlatformId::ThreadxRiscv64,
        PlatformId::Esp32Qemu,
        PlatformId::QemuBaremetal,
        PlatformId::Fvp,
        PlatformId::Px4,
    ];
}

/// RMW axis.
///
/// Issue 0341 — `Uorb` is declared supported in ARCHITECTURE §2
/// (`rmw-{zenoh,xrce,cyclonedds,uorb}`) with a real crate
/// (`packages/rmw/uorb/nros-rmw-uorb`) and example
/// (`packages/testing/nros-px4-register-check`), so it
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

    /// The axis token — the spelling used in fixture paths, cell labels and
    /// failure messages.
    ///
    /// phase-373 W3: eight tests carried a byte-identical private `lang_str`
    /// for this. A pure mapping over an enum this crate owns belongs on the
    /// enum, where a new variant makes the compiler ask about it once rather
    /// than leaving eight copies to be found by hand.
    ///
    /// Note this is deliberately NOT `Display`: the sibling `Rmw` mapping is
    /// genuinely ambiguous (`Cyclonedds` is spelled `"cyclone"` by the native
    /// example consumers and `"cyclonedds"` by `zephyr.rs`), and giving one
    /// axis a blessed `Display` while its neighbour keeps per-consumer
    /// spellings would imply an agreement that does not exist.
    pub const fn as_str(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::Mixed => "mixed",
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
    /// Platform-ABI capability, no messaging: two tasks spawned through
    /// `nros_platform_task_init` prove `errno` is PER-THREAD (issue 0680).
    /// Single image, self-contained, no peer.
    Errno,
    /// issue 1384 — a node created through `rclc_executor_node_init` (bound to
    /// an executor, `support` left NULL on purpose) BOOTS: it creates its guard
    /// condition, its eager publisher and its timer, and publishes.
    ///
    /// Its own workload rather than a case of [`Workload::Pubsub`], for the
    /// same reason [`Workload::Errno`] is not one: there is no peer and nothing
    /// to deliver to. The subject is the node→executor BINDING, which fails
    /// entirely inside one process — `rclc_publisher_init_default` answered
    /// `RCL_RET_NOT_INIT` for four phases and the only in-tree caller
    /// (`examples/linux/c/custom-platform`) was a build-only fixture, so
    /// nothing ran the binary and nothing saw it. A pubsub cell would not have
    /// caught it either: every pubsub example uses the LEGACY
    /// `rclc_node_init_default`, which was never affected.
    ExecutorBoundNode,
    /// issue 1795 — the RTOS TYPED CARRIER build road
    /// (`nros_components_register_node(... TYPED DEPLOY <rtos>)`) produces an
    /// image that BOOTS and publishes. Its own workload because its subject is
    /// the road, not delivery: the carrier synthesises a C++ entry holding the
    /// component's node as a static object, and that link failed twice (issues
    /// 1742, 1753) while every RTOS C/C++ cell took `nano_ros_entry()`. No peer;
    /// the image's own publish line is the evidence.
    TypedCarrier,
    /// phase-381 — READ the ROS graph. The node enumerates a live stock
    /// `rmw_zenoh_cpp` peer and is compared against `ros2 node list`.
    ///
    /// Its own workload because it is the only one whose subject is DISCOVERY
    /// rather than delivery: every other cell asks "did the message arrive",
    /// this one asks "can we see who is there", and the two fail for different
    /// reasons.
    Graph,
    /// phase-433 W6 — what a node ADVERTISES about itself, cross-checked
    /// against what a live ROS 2 peer reads for the same entities: matched
    /// counts, the publisher GID, the actual-QoS read-back (issue 0823) and
    /// `get_serialization_format`.
    ///
    /// Its own workload for the same reason [`Workload::Graph`] is: the
    /// subject is neither delivery nor discovery but SELF-REPORT. Every slot
    /// in it is one nobody can check from inside one process — a matched count
    /// is a statement about a peer, a GID's whole purpose is that another
    /// participant recognises it, and a granted QoS is only interesting where
    /// it differs from the requested one.
    AdvertisedState,
    /// phase-433 W6 — a QoS STATUS EVENT fires and we report it. The node
    /// registers `on_liveliness_changed` and a stock ROS 2 peer's `@ros2_lv`
    /// liveliness token is what changes state.
    ///
    /// Its own workload rather than a case of [`Workload::Qos`]: that one is
    /// about the PROFILE a peer reads back off our endpoint (`qos_override_e2e`
    /// reads `ros2 topic info --verbose`), a static property settled at
    /// creation. This one is about a NOTIFICATION raised later, by a remote
    /// entity's state change, through four vtable slots the profile lanes never
    /// touch. They fail for entirely different reasons.
    QosEvents,
    /// phase-482 W3 — a ported ROS 2 program RUNS: an upstream C++ source,
    /// vendored unmodified (`int main(int argc, char** argv)` included), built
    /// against nano-ros with only the build glue changed, publishes.
    ///
    /// Its own workload rather than a case of [`Workload::Pubsub`] because the
    /// subject is the PORT, not delivery: what fails here is a build road that
    /// cannot reach a ported `main` (no entry on a board whose startup calls
    /// `nros_app_main`), a TU that never sees the locator the board bakes (a
    /// Zephyr guest dialling its own loopback), or a toolchain that compiles the
    /// ported file freestanding. A pubsub example is written FOR nano-ros and
    /// meets none of those. The sources are `examples/templates/*-port*`.
    ///
    /// This value is `cpp-port-minimal-publisher`. The other two port
    /// templates are their own values, [`Workload::PortSmoke`] and
    /// [`Workload::PortMonitor`], because a cell is one coordinate and the three
    /// assert different behaviour.
    Port,
    /// phase-482 W3 — `examples/templates/rclcpp-compat-smoke`, ported
    /// unmodified: an `rclcpp::Node` subclass whose `main` drives a
    /// `rclcpp::spin_some` + `std::this_thread::sleep_for` loop, publishing
    /// `std_msgs/Int32` and, through `diagnostic_updater::Updater`, a
    /// `DiagnosticArray`. A host `int32-sink` must RECEIVE the Int32 topic and
    /// a host diagsink must receive the diagnostics task.
    ///
    /// A [`Workload::Port`] sibling rather than a case of it: it reaches the
    /// two pieces of the hosted C++ library the tutorial publisher never
    /// touches, `std::this_thread::sleep_for` and `std::chrono::steady_clock`,
    /// which the updater's rate limit reads. On FreeRTOS both reach newlib
    /// calls the board did not provide until this cell ran.
    PortSmoke,
    /// phase-482 W3 — `examples/templates/topic-state-monitor-port`, ported
    /// unmodified: two capturing-lambda subscriptions feed a per-topic
    /// liveness report on `/diagnostics`. The cell must see each watched topic
    /// reported STALE while no peer publishes, and LIVE (with the peer's value)
    /// once the host `int32-source` publishes both topics.
    ///
    /// Its own value because its subject is the SUBSCRIBE half of a ported
    /// node (capturing lambdas, dispatch from `rclcpp::spin_some`) plus a
    /// monotonic clock that really advances. The stale-then-live order is what
    /// keeps that honest: a clock stuck at zero reports every topic live
    /// whether or not anything arrived.
    PortMonitor,
    /// issue 1036 — a console-less image exhausts its executor arena on
    /// purpose, and the boot report it leaves in RAM names the shortfall and
    /// the knob. The record is dumped from the halted guest and decoded with
    /// `scripts/read-boot-report.py`, the tool an operator runs on a board.
    ///
    /// Its own value because nothing is delivered and nothing prints: the
    /// image has no console, which is the condition the record exists for.
    /// The verdict is read out of memory, not out of a stream.
    BootReport,
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
            // Needs no port: the fixture opens no socket and has no peer.
            // The offset only has to be unique within the band.
            Workload::Errno => 94,
            // Same reason as Errno: no peer dials this image.
            Workload::ExecutorBoundNode => 99,
            // issue 1795. 81..=87 is free in every band (see PortSmoke below);
            // zenoh only, so the domain window is irrelevant.
            Workload::TypedCarrier => 81,
            Workload::Graph => 95,
            Workload::AdvertisedState => 96,
            // Needs no port of its own: the fixture's peer is reached over the
            // cell's zenoh locator. Unique within the band, which is all the
            // offset has to be.
            Workload::QosEvents => 97,
            Workload::Port => 98,
            // The two other port templates (phase-482 W3). 81..=89 is free in
            // every platform's band; these do not reach the domain window
            // (`domain_of` clamps every offset past 60), and every port cell
            // is zenoh.
            Workload::PortSmoke => 88,
            Workload::PortMonitor => 89,
            // A router port of its own: the image opens its session before
            // it registers the entity that exhausts the arena (issue 1036).
            Workload::BootReport => 87,
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

/// Configuration axis (RFC-0051 §7a, phase-510): a build-time switch that
/// changes the MECHANISM a workload runs through, not what it observes. A cell
/// carries a SET of them (`Cell::config`), empty for the default build.
///
/// A `Workload` changes what the test asserts; a `Config` must not — a lending
/// pubsub delivers exactly what plain pubsub does. The variants are the
/// `class = "config"` rows of `.config/feature-lanes.toml`, and
/// `just check required-features-reachable` holds the two to the same set.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Config {
    /// Loan publish (`SlotLending`): the payload is written into a slot the
    /// backend lends, instead of copied from a caller buffer (issue 0814).
    Lending,
    /// A zenoh-pico session pool larger than one (`NROS_ZENOH_MAX_SESSIONS`).
    Multisession,
    /// The executor wakes through a signal fd instead of polling.
    SignalFdWake,
    /// zenoh-pico over a caller-supplied link instead of its own sockets.
    CustomTransport,
}

impl Config {
    pub const fn as_str(self) -> &'static str {
        match self {
            Config::Lending => "lending",
            Config::Multisession => "multisession",
            Config::SignalFdWake => "signal-fd-wake",
            Config::CustomTransport => "custom-transport",
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
    /// The build configurations the cell's image is built with (RFC-0051
    /// §7a). Empty is the default build; a configured cell is backed by a
    /// fixture row that enables them and asserts what they are for.
    pub config: &'static [Config],
}

impl Cell {
    /// This cell, built with `config` (RFC-0051 §7a).
    pub const fn with(self, config: &'static [Config]) -> Cell {
        Cell { config, ..self }
    }
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
        config: &[],
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
/// ESP32 is DORMANT (issue 1525): the board, platform crate and examples stay
/// in the tree for future use, but no lane, fixture row or `just` module builds
/// or tests them.
const ESP32_DORMANT: &str =
    "ESP32 support is dormant (issue 1525): code kept, nothing builds or tests it";

/// phase-510 W3 — a workspace entry that BUILDS (its fixture row is in every
/// lane that needs it) but whose DELIVERY no test asserts: the only runtime
/// touch is `rmw_coordinate_truth`, which checks that the entry registers its
/// node on the right RMW — registration, not a sample arriving. It was
/// `Runtime` until a cell had to name the test that answers it (RFC-0051
/// §7c). Unlocked by a consumer that boots the entry and counts deliveries.
const NO_DELIVERY_TEST: &str = "builds; only rmw_coordinate_truth boots it (node registration, \
     not delivery) — needs a consumer that asserts delivery (phase-510 W3)";
/// phase-510 W3 — `entry_e2e` claimed this cell and booted the ZENOH image:
/// its exec table is keyed without the RMW, so `build_zephyr_workspace_cpp_entry`
/// answered for the Cyclone row too, and `workspace-zephyr-cpp-cyclonedds` was
/// resolved by no test. Unlocked by a Cyclone arm in that table.
const ENTRY_E2E_IS_ZENOH_ONLY: &str = "builds; entry_e2e's exec table has no Cyclone arm (it booted \
     the zenoh image for this cell) — phase-510 W3";
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
    // Linux host (ephemeral isolation; all three RMWs have runtime lanes).
    cell(Linux, Rust, Zenoh,      Pubsub,  Example, Runtime),
    // phase-510 W4 — the same pubsub BUILT with loan publish in a two-session
    // pool (RFC-0051 §7a): `loan_e2e` publishes through `try_loan` + `commit`
    // and its subscriber, in the same process, must receive what it would
    // from a plain publish. Answered by `test-zpico-multisession`'s lane.
    cell(Linux, Rust, Zenoh,      Pubsub,  Example, Runtime).with(&[Config::Lending, Config::Multisession]),
    cell(Linux, C,    Zenoh,      Pubsub,  Example, Runtime),
    cell(Linux, Cpp,  Zenoh,      Pubsub,  Example, Runtime),
    // issue 1384 — `examples/linux/c/custom-platform`, the one in-tree caller
    // of `rclc_executor_node_init` that creates an eager entity. Its
    // fixtures.toml row was BUILD-ONLY, so nothing ran the binary and its
    // publisher failed RCL_RET_NOT_INIT unobserved for four phases.
    cell(Linux, C,    Zenoh,      ExecutorBoundNode, Example, Runtime),
    cell(Linux, Rust, Zenoh,      Service, Example, Runtime),
    cell(Linux, C,    Zenoh,      Service, Example, Runtime),
    cell(Linux, Cpp,  Zenoh,      Service, Example, Runtime),
    cell(Linux, Rust, Zenoh,      Action,  Example, Runtime),
    cell(Linux, C,    Zenoh,      Action,  Example, Runtime),
    cell(Linux, Cpp,  Zenoh,      Action,  Example, Runtime),
    // phase-482 W3 — the unmodified ROS 2 tutorial publisher
    // (`examples/templates/cpp-port-minimal-publisher`), run by
    // `port_templates_e2e`. Same program on all three platforms; only the
    // build glue differs.
    cell(Linux, Cpp,  Zenoh,      Port,    Example, Runtime),
    // phase-482 W3 — the other two port templates, the same way:
    // `rclcpp-compat-smoke` and `topic-state-monitor-port`, each one source on
    // all three platforms, run by `port_templates_e2e`.
    cell(Linux, Cpp,  Zenoh,      PortSmoke,   Example, Runtime),
    cell(Linux, Cpp,  Zenoh,      PortMonitor, Example, Runtime),
    cell(Linux, Rust, Cyclonedds, Pubsub,  Example, Runtime),
    cell(Linux, C,    Cyclonedds, Pubsub,  Example, Runtime),
    cell(Linux, Cpp,  Cyclonedds, Pubsub,  Example, Runtime),
    cell(Linux, C,    Cyclonedds, Service, Example, Runtime),
    cell(Linux, Cpp,  Cyclonedds, Service, Example, Runtime),
    // issue #233 cell 1 — proven: rust cyclone service pair delivers
    // (test_native_cyclonedds_rust_service).
    cell(Linux, Rust, Cyclonedds, Service, Example, Runtime),
    cell(Linux, C,    Cyclonedds, Action,  Example, Runtime),
    cell(Linux, Cpp,  Cyclonedds, Action,  Example, Runtime),
    // issue #234 — RESOLVED: rust cyclone action pair delivers the order-10
    // Fibonacci result (test_native_cyclonedds_rust_action). The action's
    // `register_protocol_types` now registers the `action_msgs` descriptors
    // through the generic `nros_rmw::register_type_descriptor` seam instead of
    // the cfg-gated named-backend call that compiled out of the example build.
    cell(Linux, Rust, Cyclonedds, Action,  Example, Runtime),
    cell(Linux, C,    Xrce,       Pubsub,  Example, Runtime),
    cell(Linux, Rust, Xrce,       Pubsub,  Example, Runtime),
    cell(Linux, Cpp,  Xrce,       Pubsub,  Example, Runtime),
    cell(Linux, C,    Xrce,       Service, Example, Runtime),
    cell(Linux, Rust, Xrce,       Service, Example, Runtime),
    cell(Linux, Cpp,  Xrce,       Service, Example, Runtime),
    cell(Linux, C,    Xrce,       Action,  Example, Runtime),
    cell(Linux, Rust, Xrce,       Action,  Example, Runtime),
    cell(Linux, Cpp,  Xrce,       Action,  Example, Runtime),

    // Zephyr on QEMU mps2/an385 (Cortex-M3) — phase-337 W2.d. The point of
    // these rows is everything the native_sim rows below CANNOT say: 32-bit
    // pointers, Zephyr's own in-kernel IP stack, and a real ethernet driver
    // (`eth_smsc911x` against QEMU's lan9118). Proven end to end in W2.b — the
    // image boots, takes 10.0.2.15 from the driver, and publishes over a zenoh
    // session to a host router through SLIRP.
    //
    // NO RUST ROW, deliberately, and NOT as a BuildOnly: the pinned
    // `zephyr-lang-rust` cannot compile the `zephyr` crate for any board whose
    // devicetree has gpio nodes (issue 0432), so a rust cell here would not
    // build either, and `BuildOnly` promises that it does. An absent row with
    // the reason recorded beats a tier that lies — see this module's header.
    //
    // Zenoh only: cyclone and xrce on this board are untried, and the honest
    // record of "nobody has attempted it" is no row, not a guess.
    cell(ZephyrQemuCortexM, C,   Zenoh, Pubsub,  Example, Runtime),
    cell(ZephyrQemuCortexM, Cpp, Zenoh, Pubsub,  Example, Runtime),
    // phase-482 W3 — the ported tutorial publisher on Zephyr. On this board,
    // not native_sim: a ported program needs the full libstdc++, which
    // native_sim's C library cannot carry (phase-209 G.2).
    cell(ZephyrQemuCortexM, Cpp, Zenoh, Port,    Example, Runtime),
    cell(ZephyrQemuCortexM, Cpp, Zenoh, PortSmoke,   Example, Runtime),
    cell(ZephyrQemuCortexM, Cpp, Zenoh, PortMonitor, Example, Runtime),
    // phase-346 W3 — Rust on a REAL Zephyr board, unblocked by patching issue
    // 0432's two upstream defects. Until then the `zephyr` crate could not
    // compile for any board whose devicetree has gpio nodes, which is every
    // real one; native_sim has none, which is the only reason it went unseen.
    cell(ZephyrQemuCortexM, Rust, Zenoh, Pubsub,  Example, Runtime),
    // issue 1036 — the boot report read out of a console-less, halted 32-bit
    // image. On this board because it is Zephyr's own IP stack on a Cortex-M,
    // the closest QEMU has to the MR-CANHUBK344 the record was written for.
    cell(ZephyrQemuCortexM, C,   Zenoh, BootReport, Example, Runtime),
    cell(ZephyrQemuCortexM, C,   Zenoh, Service, Example,
         BuildOnly("phase-337 W2.f — the runner reproduces W2.b's manual setup \
                    (SLIRP + a host zenoh router on 7456) for pubsub only; \
                    service needs the peer half of that harness")),
    cell(ZephyrQemuCortexM, Cpp, Zenoh, Service, Example,
         BuildOnly("phase-337 W2.f — same runner gap as the C service row")),
    cell(ZephyrQemuCortexM, C,   Zenoh, Action,  Example,
         BuildOnly("phase-337 W2.f — same runner gap as the C service row")),
    cell(ZephyrQemuCortexM, Cpp, Zenoh, Action,  Example,
         BuildOnly("phase-337 W2.f — same runner gap as the C service row")),

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
    // phase-482 W3 — the ported tutorial publisher on FreeRTOS.
    cell(FreertosMps2, Cpp,  Zenoh, Port,    Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, PortSmoke,   Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, PortMonitor, Example, Runtime),
    cell(FreertosMps2, Rust, Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, C,    Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, Service, Example, Runtime),
    cell(FreertosMps2, Rust, Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, C,    Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, Action,  Example, Runtime),
    cell(FreertosMps2, Rust, Cyclonedds, Pubsub, Example,
         CarveOut("no fixture builds it: the cmake-bridge entry was retired in \
                   phase-220.C and issue #233 closed without restoring it. \
                   Reviving it is a Cyclone cmake entry for the rust example, a \
                   fixtures.toml row and a runtime test (the two retired \
                   freertos_qemu.rs tests are in git history)")),
    cell(FreertosMps2, Rust, Xrce,       Pubsub, Example, CarveOut(XRCE_RTOS_CARVE)),

    cell(NuttxArm, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Pubsub,  Example, Runtime),
    // issue 1795 — the NuttX TYPED carrier road boots and publishes
    // (`examples/qemu-armv7a-nuttx/c/typed-carrier`, tests/typed_carrier_e2e.rs).
    cell(NuttxArm, C,    Zenoh, TypedCarrier, Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(NuttxArm, Rust, Zenoh, Service, Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Service, Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Service, Example, Runtime),
    cell(NuttxArm, Rust, Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, C,    Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, Cpp,  Zenoh, Action,  Example, Runtime),
    cell(NuttxArm, Rust, Cyclonedds, Pubsub, Example, CarveOut(CYCLONE_RUST_RTOS_CARVE)),
    cell(NuttxArm, Rust, Xrce,       Pubsub, Example, CarveOut(XRCE_RTOS_CARVE)),
    // issue 1706 — the parameter store CARVED from the executor backing on a
    // booted image (`bins/param-store-nuttx-qemu-arm`, tests/param_store_carve.rs).
    cell(NuttxArm, Rust, Zenoh, Params, Example, Runtime),

    cell(ThreadxLinux, Rust, Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Pubsub,  Example, Runtime),
    cell(ThreadxLinux, Rust, Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Service, Example, Runtime),
    cell(ThreadxLinux, Rust, Zenoh, Action,  Example, Runtime),
    cell(ThreadxLinux, C,    Zenoh, Action,  Example, Runtime),
    cell(ThreadxLinux, Cpp,  Zenoh, Action,  Example, Runtime),
    // threadx-linux cyclone: C pubsub pair proven (linux_api #215 lane);
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
    // issue 0680 — per-thread `errno`. Not a messaging workload: the fixture
    // spawns two tasks and asserts one cannot see the other's `errno`. It is a
    // cell so the fixture has an owner that RUNS it; before the fix it fails
    // with `FAIL shared errno`, which is what makes it worth a coordinate.
    cell(ThreadxRiscv64, C,    Zenoh, Errno, Example, Runtime),
    // issue 1795 — the ThreadX TYPED carrier road boots and publishes
    // (`examples/rv-virt-threadx/c/typed-carrier`, tests/typed_carrier_e2e.rs).
    cell(ThreadxRiscv64, C,    Zenoh, TypedCarrier, Example, Runtime),
    cell(ThreadxRiscv64, C,    Cyclonedds, Pubsub, Example, Runtime),
    cell(ThreadxRiscv64, Rust, Cyclonedds, Pubsub, Example, Runtime),
    // issue #235 — the cpp cyclone riscv64 fixtures existed (distinct
    // identity per node); the two-QEMU lane
    // (test_threadx_riscv64_cyclonedds_two_qemu_cpp_pubsub) now consumes them.
    cell(ThreadxRiscv64, Cpp,  Cyclonedds, Pubsub, Example, Runtime),
    // issue 1706 — the parameter store CARVED from the executor backing on a
    // booted image (`bins/param-store-threadx-riscv64`, tests/param_store_carve.rs).
    cell(ThreadxRiscv64, Rust, Zenoh, Params, Example, Runtime),

    // NuttX riscv — the C talker example has a runtime lane
    // (c_riscv_nuttx_e2e); rust/cpp have NO standalone pubsub examples —
    // their riscv coverage is the realtime-tiers WORKSPACE lanes (rows
    // below), so don't claim Example-Runtime here.
    cell(NuttxRiscv, C,    Zenoh, Pubsub, Example, Runtime),
    cell(NuttxRiscv, Cpp,  Zenoh, Pubsub, Example,
         CarveOut("no standalone cpp pubsub example on rv-virt; runtime coverage rides the realtime-tiers workspace lane")),
    cell(NuttxRiscv, Rust, Zenoh, Pubsub, Example,
         CarveOut("no standalone rust pubsub example on rv-virt; runtime coverage rides the realtime-tiers workspace lane")),

    // ESP32 is DORMANT (issue 1525): its code is kept for future use, and no
    // lane builds or runs it, so every cell is a carve-out with no fixture row.
    cell(Esp32Qemu, Rust, Zenoh, Pubsub,  Example, CarveOut(ESP32_DORMANT)),
    cell(Esp32Qemu, Rust, Zenoh, Service, Example, CarveOut(ESP32_DORMANT)),
    cell(Esp32Qemu, Rust, Zenoh, Action,  Example, CarveOut(ESP32_DORMANT)),
    cell(Esp32Qemu, C,    Zenoh, Pubsub,  Example, CarveOut(ESP32_DORMANT)),
    cell(Esp32Qemu, Cpp,  Zenoh, Pubsub,  Example, CarveOut(ESP32_DORMANT)),

    // Bare-metal RTIC (QEMU MPS2) — pubsub-only demo set by design.
    cell(QemuBaremetal, Rust, Zenoh, Pubsub, Example, Runtime),
    cell(QemuBaremetal, Rust, Zenoh, Service, Example,
         CarveOut("rtic demo set is pubsub-only by design (phase-289 scope)")),
    cell(QemuBaremetal, Rust, Zenoh, Action, Example,
         CarveOut("rtic demo set is pubsub-only by design (phase-289 scope)")),
    // issue 1512 — the C/C++ API on the same board. Cargo-rooted leaves
    // (`builder = "cargo"` rows): the Rust startup owns the link and compiles
    // the C/C++ application in. RUNTIME since the image's declared locator
    // reaches the C/C++ compile (`nros_cc_flags::entry_config`) and the boot
    // `Config` (`Config::from_image`); each leaf's `[image.*] locator` names
    // its cell's `alloc::port`, and `tests/emulator.rs`
    // (`test_qemu_baremetal_capi_e2e`) runs every pair. The action pairs
    // joined once the smoltcp read stopped desynchronising on a burst of
    // batches (issue 1797).
    cell(QemuBaremetal, C,   Zenoh, Pubsub,  Example, Runtime),
    cell(QemuBaremetal, C,   Zenoh, Service, Example, Runtime),
    cell(QemuBaremetal, C,   Zenoh, Action,  Example, Runtime),
    cell(QemuBaremetal, Cpp, Zenoh, Pubsub,  Example, Runtime),
    cell(QemuBaremetal, Cpp, Zenoh, Service, Example, Runtime),
    cell(QemuBaremetal, Cpp, Zenoh, Action,  Example, Runtime),

    // phase-337 W7.a — the three `Stm32F4` BuildOnly cells left with the board.
    // They were `BuildOnly`, i.e. ZERO Runtime, so the removal costs no
    // coverage: `check-board-tiers` requires a Runtime cell for tier 1/2 and
    // this platform could never have one (no hardware in the rack, no QEMU
    // model of the F4 MAC). The board is now
    // `book/src/porting/stm32f4-out-of-tree.md`.

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
    cell(Linux, Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(Linux, C,     Zenoh, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, Cpp,   Zenoh, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, Mixed, Zenoh, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    // phase-331 W4 (RFC-0066) — the RMW AXIS over the language workspaces.
    // Before this the workspace rows were 85/87 zenoh: the RMW seam was
    // exercised by single-node examples and essentially never by a workspace.
    // `mixed` is deliberately absent — its value is the language seam, not the
    // RMW seam. The rust cells already existed; only c/cpp were missing.
    cell(Linux, Rust, Cyclonedds, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, C,    Cyclonedds, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, Cpp,  Cyclonedds, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, Rust, Xrce,       EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, C,    Xrce,       EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(Linux, Cpp,  Xrce,       EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(ZephyrNativeSim, Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, C,     Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, Cpp,   Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ZephyrNativeSim, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    // phase-206 W2 — the RTOS image that links Cyclone AND carries a baked
    // bringup config (`workspace-zephyr-cpp-cyclonedds`). Its fixture row
    // landed with commit 9ae271174 while this cell did not, so the coordinate
    // (zephyr, cpp, cyclonedds, workspace) was a fixtures.toml orphan: built
    // and run by the lane, modeled by nothing. `fixture_rows_all_modeled_by_matrix`
    // is the assertion that caught it, and it is the reverse direction of the
    // pair -- the forward one was green throughout, because a cell that does
    // not exist cannot be missing a fixture.
    cell(ZephyrNativeSim, Cpp,   Cyclonedds, EntryPubsub, Workspace, BuildOnly(ENTRY_E2E_IS_ZENOH_ONLY)),
    cell(FreertosMps2, C,    Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Cpp,  Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Rust, Zenoh, EntryPubsub, Workspace, Runtime),
    // phase-370 W3 — the freertos family's first cells that run without QEMU.
    // The POSIX simulator is a host process, so `Runtime` here costs a process
    // spawn rather than an emulator, and the RMW is the same host CycloneDDS
    // the `Linux` rows use. No Rust row: the POSIX port has no Rust entry shape
    // of its own and the board carries no Rust board crate.
    cell(FreertosPosix, C,   Cyclonedds, EntryPubsub, Workspace, Runtime),
    cell(FreertosPosix, Cpp, Cyclonedds, EntryPubsub, Workspace, Runtime),
    // phase-372 — the S32Z270 Cortex-R52 bundle's workspace row
    // (`workspace-cpp-s32z270-freertos`). It borrows the `freertos` platform
    // token for the family lane; MPS2 owns that platform's Runtime cells, and
    // this board has no witness of its own, so it is BuildOnly rather than a
    // promise CI runs. The string is what unlocks it.
    cell(FreertosMps2, Cpp, Cyclonedds, EntryPubsub, Workspace,
         BuildOnly("phase-372 — S32Z270 is link-complete only: the NXP GIC port, \
                    RTD NETC driver and PBcfg are consumer-provisioned seams \
                    (autoware-safety-island), so no in-tree runner can boot it. \
                    Unlocks when a hardware or simulator witness exists")),
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
    cell(ThreadxLinux, Rust,  Zenoh, EntryPubsub, Workspace, BuildOnly(NO_DELIVERY_TEST)),
    cell(ThreadxLinux, C,     Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ThreadxLinux, Cpp,   Zenoh, EntryPubsub, Workspace, Runtime),
    cell(ThreadxLinux, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    // issue 1286 follow-up — the pure-C ThreadX entry off the host sim:
    // rv-virt (riscv64 QEMU virt, slirp), `workspace-c-threadx-riscv64`.
    cell(ThreadxRiscv64, C, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(FreertosMps2, Mixed, Zenoh, EntryPubsub, Workspace, Runtime),
    cell(NuttxArm,     Rust,  Zenoh, EntryPubsub, Workspace, Runtime),
    // See the nuttx-riscv correction above — the rust riscv workspace row
    // is realtime-only too (workspace-rust-nuttx-riscv-realtime); no
    // EntryPubsub image or lane exists. phase-295 W3.b finding.
    cell(NuttxRiscv,   Rust,  Zenoh, EntryPubsub, Workspace,
         BuildOnly("no nuttx-riscv rust EntryPubsub workspace fixture/lane (RT-tiers \
                    only) — phase-295 W3.b finding, W6 wires it")),
    cell(Esp32Qemu,    Rust, Zenoh, EntryPubsub, Workspace, CarveOut(ESP32_DORMANT)),

    // Workspace feature workloads (native + zephyr today; per-lang rows
    // mirror the ws-* families).
    cell(Linux, C,     Zenoh, CustomMsg, Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, CustomMsg, Workspace, Runtime),
    // Corrected during the phase-295 W3.b consolidation: the seed table
    // marked native rust CustomMsg/Qos `Runtime`, but no fixtures.toml row
    // builds `ws-{custom-msg,qos}-rust`'s `linux_entry` and no test
    // consumes it (the C files' "C projection of the Rust demo" prose
    // described the WORKSPACE, not a lane; ws-qos-rust's only runtime lane
    // is the zephyr image). Single-entry natives also hit issue 0096
    // (in-process pub→sub never delivers), so wiring them needs split
    // talker/listener entries first — issue #233.
    cell(Linux, Rust,  Zenoh, CustomMsg, Workspace,
         BuildOnly("ws-custom-msg-rust linux_entry has no fixture row or runtime lane \
                    (needs an 0096 two-entry split) — phase-295 W3.b finding, W6 wires it")),
    cell(Linux, Mixed, Zenoh, CustomMsg, Workspace, Runtime),
    cell(Linux, C,     Zenoh, Logging,   Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Logging,   Workspace, Runtime),
    // Added during the phase-295 W3.b consolidation: the rust + mixed
    // logging lanes existed (tests/{,mixed_}logging_workspace_e2e.rs,
    // phase-263 A5) but the seed table never modeled them.
    cell(Linux, Rust,  Zenoh, Logging,   Workspace, Runtime),
    cell(Linux, Mixed, Zenoh, Logging,   Workspace, Runtime),
    cell(Linux, C,     Zenoh, Qos,       Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Qos,       Workspace, Runtime),
    // See the CustomMsg rust row above — same phase-295 W3.b correction.
    cell(Linux, Rust,  Zenoh, Qos,       Workspace,
         BuildOnly("ws-qos-rust linux_entry has no fixture row or runtime lane (only \
                    the zephyr image is consumed) — phase-295 W3.b finding, W6 wires it")),
    cell(Linux, Mixed, Zenoh, Qos,       Workspace, Runtime),
    cell(Linux, C,     Zenoh, Params,    Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Params,    Workspace, Runtime),
    cell(Linux, Rust,  Zenoh, Params,    Workspace, Runtime),
    cell(Linux, C,     Zenoh, Lifecycle, Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Lifecycle, Workspace, Runtime),
    cell(Linux, Rust,  Zenoh, Lifecycle, Workspace, Runtime),
    cell(Linux, C,     Zenoh, Safety,    Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Safety,    Workspace, Runtime),
    cell(Linux, Rust,  Zenoh, Safety,    Workspace, Runtime),
    // phase-306 W4 (issue 0255) — launch/model remap + `~` private name hits
    // the wire remapped. Rust only: the C/C++ `nros_cpp_declare_remap` path is
    // emitter-unit-tested (W3); a runtime C/C++ cell is residual.
    cell(Linux, Rust,  Zenoh, Remap,     Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Params,    Workspace, Runtime),
    // Issue 1681 -- the C++ params entry: the one Zephyr C/C++ image that
    // declares `param_services` (+ `lifecycle`), which did not link before.
    cell(ZephyrNativeSim, Cpp,  Zenoh, Params,    Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Lifecycle, Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Qos,       Workspace, Runtime),
    cell(ZephyrNativeSim, Rust, Zenoh, Safety,    Workspace, Runtime),

    // Realtime tiers + multihost.
    cell(Linux, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Linux, C,    Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Linux, Cpp,  Zenoh, RealtimeTiers, Workspace, Runtime),
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
    // issue 0636 gap 2 — the Rust arm. `run_tiers_entry` was exported and
    // reachable and called by nothing, so the boot-tier fix on this board was
    // reasoned onto Rust rather than measured. This cell measures it.
    cell(FreertosMps2, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(FreertosMps2, Cpp, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(FreertosMps2, C,   Zenoh, RealtimeTiers, Workspace, Runtime),
    // phase-297 W5 (RFC-0053) — ThreadX multi-tier run_tiers acceptance:
    // hosted simulation (pthread-backed ThreadX, host binary + NSOS host
    // sockets), port 9091 = port_of(ThreadxLinux, Rust, RealtimeTiers).
    cell(ThreadxLinux, Rust, Zenoh, RealtimeTiers, Workspace, Runtime),
    cell(Linux, Rust,  Zenoh, Multihost, Workspace, Runtime),
    cell(Linux, C,     Zenoh, Multihost, Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Multihost, Workspace, Runtime),
    cell(Linux, Mixed, Zenoh, Multihost, Workspace, Runtime),
    // The embedded multihost lane is the RUST robot1 zephyr image (276 W6);
    // corrected from Cpp during the phase-295 W3.b consolidation.
    cell(ZephyrNativeSim, Rust, Zenoh, Multihost, Workspace, Runtime),

    // Cross-process service/action roundtrips (phase-263 A1/A4; issue 0096
    // forces the two-process topology) — tests/roundtrip_xprocess_e2e.rs.
    cell(Linux, Rust,  Zenoh, Service, Workspace, Runtime),
    cell(Linux, C,     Zenoh, Service, Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Service, Workspace, Runtime),
    cell(Linux, Mixed, Zenoh, Service, Workspace, Runtime),
    cell(Linux, Rust,  Zenoh, Action,  Workspace, Runtime),
    cell(Linux, C,     Zenoh, Action,  Workspace, Runtime),
    cell(Linux, Cpp,   Zenoh, Action,  Workspace, Runtime),
    cell(Linux, Mixed, Zenoh, Action,  Workspace, Runtime),

    // Workspace RMW variants (thin today: 80/82 rows are zenoh — issue #233).

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

/// Does any cell of `p` ask for something to be BUILT (Runtime or BuildOnly)?
///
/// A platform whose every cell is a `CarveOut` — PX4 (no runner builds SITL)
/// and ESP32 (dormant, issue 1525) — has nothing for a fixture row, a `just`
/// module or a lane to produce. The coverage gates exempt it by this PROPERTY,
/// derived from `CELLS`, rather than by naming it a second time.
pub fn platform_is_built(p: PlatformId) -> bool {
    CELLS
        .iter()
        .any(|c| c.platform == p && !matches!(c.tier, Tier::CarveOut(_)))
}

/// Runtime cells only — what the matrix consumers iterate.
pub fn runtime_cells() -> impl Iterator<Item = &'static Cell> {
    CELLS.iter().filter(|c| matches!(c.tier, Tier::Runtime))
}

/// The test binary that answers each Runtime cell — RFC-0051 §7c, phase-510
/// W3 (which generalised phase-329 W1's five-file `W1Consumer`).
///
/// Every Runtime cell has exactly ONE consumer ([`consumer_of`] is total over
/// Runtime cells, gated by `every_runtime_cell_has_a_consumer`), and every
/// consumer binds its cases to the cells it is assigned, in one of two ways:
///
/// * it DERIVES its cases from `CELLS.filter(|c| consumer_of(c) ==
///   Some(..))`, so adding a cell makes it run (the phase-329 W1 shape:
///   multihost, roundtrip, realtime tiers, entry, workspace features); or
/// * it hand-lists them and asserts the list with [`assert_consumer_cases`],
///   so a drift either way is a red, not a cell nobody runs.
///
/// `matrix_fixture_coverage::every_test_binary_is_a_consumer_an_interop_test_or_classified`
/// holds the other half: every `tests/*.rs` binary is a consumer, an interop
/// cell's test, or a [`NON_CELL_TESTS`] row with its reason.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Consumer {
    /// `tests/multihost_e2e.rs` — every `Multihost` cell.
    Multihost,
    /// `tests/roundtrip_xprocess_e2e.rs` — workspace `Service`/`Action`.
    Roundtrip,
    /// `tests/realtime_tiers_e2e.rs` — every `RealtimeTiers` cell.
    RealtimeTiers,
    /// `tests/entry_e2e.rs` — the RTOS zenoh `EntryPubsub` subset + zephyr
    /// feature entries.
    Entry,
    /// `tests/workspace_features_e2e.rs` — native feature workspaces.
    WorkspaceFeatures,
    LinuxExamplePubsub,
    LinuxExampleReqresp,
    LinuxExampleExecutorBoundNode,
    PortTemplates,
    ZephyrCortexM,
    Zephyr,
    RtosE2e,
    TypedCarrier,
    ParamStoreCarve,
    LinuxApi,
    ThreadxRiscv64Qemu,
    CRiscvNuttx,
    Emulator,
    DeployedNativeSystem,
    BridgeNetwork,
    FreertosPosix,
    CppCParamLiveRead,
    ParamLiveRead,
    /// `tests/loan_e2e.rs` — loan publish in a two-session process.
    LoanE2e,
}

impl Consumer {
    /// Every consumer, for the gates that iterate them.
    pub const ALL: &'static [Consumer] = &[
        Consumer::Multihost,
        Consumer::Roundtrip,
        Consumer::RealtimeTiers,
        Consumer::Entry,
        Consumer::WorkspaceFeatures,
        Consumer::LinuxExamplePubsub,
        Consumer::LinuxExampleReqresp,
        Consumer::LinuxExampleExecutorBoundNode,
        Consumer::PortTemplates,
        Consumer::ZephyrCortexM,
        Consumer::Zephyr,
        Consumer::RtosE2e,
        Consumer::TypedCarrier,
        Consumer::ParamStoreCarve,
        Consumer::LinuxApi,
        Consumer::ThreadxRiscv64Qemu,
        Consumer::CRiscvNuttx,
        Consumer::Emulator,
        Consumer::DeployedNativeSystem,
        Consumer::BridgeNetwork,
        Consumer::FreertosPosix,
        Consumer::CppCParamLiveRead,
        Consumer::ParamLiveRead,
        Consumer::LoanE2e,
    ];

    /// The test binary (`tests/<binary>.rs`).
    pub const fn binary(self) -> &'static str {
        match self {
            Consumer::Multihost => "multihost_e2e",
            Consumer::Roundtrip => "roundtrip_xprocess_e2e",
            Consumer::RealtimeTiers => "realtime_tiers_e2e",
            Consumer::Entry => "entry_e2e",
            Consumer::WorkspaceFeatures => "workspace_features_e2e",
            Consumer::LinuxExamplePubsub => "linux_example_pubsub_e2e",
            Consumer::LinuxExampleReqresp => "linux_example_reqresp_e2e",
            Consumer::LinuxExampleExecutorBoundNode => "linux_example_executor_bound_node_e2e",
            Consumer::PortTemplates => "port_templates_e2e",
            Consumer::ZephyrCortexM => "zephyr_cortex_m_qemu",
            Consumer::Zephyr => "zephyr",
            Consumer::RtosE2e => "rtos_e2e",
            Consumer::TypedCarrier => "typed_carrier_e2e",
            Consumer::ParamStoreCarve => "param_store_carve",
            Consumer::LinuxApi => "linux_api",
            Consumer::ThreadxRiscv64Qemu => "threadx_riscv64_qemu",
            Consumer::CRiscvNuttx => "c_riscv_nuttx_e2e",
            Consumer::Emulator => "emulator",
            Consumer::DeployedNativeSystem => "deployed_native_system_e2e",
            Consumer::BridgeNetwork => "bridge_network_e2e",
            Consumer::FreertosPosix => "freertos_posix",
            Consumer::CppCParamLiveRead => "cpp_c_param_live_read_e2e",
            Consumer::ParamLiveRead => "param_live_read_e2e",
            Consumer::LoanE2e => "loan_e2e",
        }
    }
}

/// Why a `tests/*.rs` binary answers no matrix cell — RFC-0051 §7c.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NonCell {
    /// Reads repo files, or runs the `nros` CLI on in-tree inputs; no fixture.
    Static,
    /// Resolves a compile / cmake / west fixture by ID and asserts a build
    /// artifact or verdict — coordinate-free.
    CompileCheck,
    /// The docker ROS-edition axis — a per-run global, not a cell (issue 0327).
    Edition,
    /// A harness-gated target in `.config/feature-lanes.toml` (RFC-0051 §7b).
    FeatureLane,
    /// Runtime, but every coordinate it touches has a different primary
    /// consumer; the string says which, and what this one adds.
    Secondary(&'static str),
    /// Runtime over a coordinate (or a workload) the matrix does not model;
    /// the string says which. Each is a candidate cell, not an exemption.
    Unmodeled(&'static str),
}

/// Every `tests/*.rs` binary that is neither a [`Consumer`] nor an interop
/// cell's test, with its reason (RFC-0051 §7c, phase-510 W3). The partition is
/// gated: a binary in none of the three, or in two, fails
/// `matrix_fixture_coverage::every_test_binary_is_a_consumer_an_interop_test_or_classified`.
pub const NON_CELL_TESTS: &[(&str, NonCell)] = &[
    (
        "action_multigoal",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Action/Example — answered by linux_example_reqresp_e2e; this one asserts concurrent goals",
        ),
    ),
    (
        "action_raw_goal_e2e",
        NonCell::Secondary(
            "Linux/C/Zenoh/Action/Example — answered by linux_example_reqresp_e2e; this one asserts the raw-goal probe",
        ),
    ),
    (
        "actions",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Action/Example — answered by linux_example_reqresp_e2e",
        ),
    ),
    (
        "baremetal_run_plan_runtime",
        NonCell::Unmodeled(
            "QemuBaremetal/Rust/Zenoh/EntryPubsub/Workspace boot — no cell models it",
        ),
    ),
    ("board_agnostic_run_plan", NonCell::CompileCheck),
    ("board_import", NonCell::CompileCheck),
    ("board_link_archives", NonCell::Static),
    (
        "borrowed_e2e",
        NonCell::Unmodeled("borrowed-view serdes probe, no RMW — not a matrix workload"),
    ),
    (
        "bridge_mixed_rmw",
        NonCell::Secondary("Linux/Rust bridge — answered by the interop cell bridge-zenoh-to-xrce"),
    ),
    ("bringup_scaffold", NonCell::Static),
    ("c_mixed_workspace", NonCell::CompileCheck),
    ("c_rosidl_refusals", NonCell::CompileCheck),
    (
        "c_xrce_api",
        NonCell::Secondary(
            "Linux/C/Xrce/Pubsub/Example — answered by linux_example_pubsub_e2e; this one exercises the C XRCE API surface",
        ),
    ),
    (
        "cli_bringup_zephyr",
        NonCell::Secondary(
            "Zephyr native_sim workspace entry boot — answered by zephyr / entry_e2e",
        ),
    ),
    ("cmake_add_subdirectory", NonCell::CompileCheck),
    ("cmake_node_register_metadata", NonCell::CompileCheck),
    ("cmake_node_register_misuse", NonCell::CompileCheck),
    ("cmake_platform_matrix", NonCell::CompileCheck),
    ("component_dispatch", NonCell::FeatureLane),
    ("component_param", NonCell::FeatureLane),
    ("component_runtime", NonCell::FeatureLane),
    (
        "condvar_contract_rtos",
        NonCell::Unmodeled(
            "the condvar platform contract on ThreadX/FreeRTOS — not a matrix workload",
        ),
    ),
    ("context_create_executor", NonCell::FeatureLane),
    ("contract_derived_chain_parity", NonCell::Static),
    (
        "contract_monitor_parity",
        NonCell::Unmodeled("the contract monitor — not a matrix workload"),
    ),
    ("cpp_api_drift", NonCell::Static),
    (
        "cpp_lifecycle_node_wrapper_e2e",
        NonCell::Secondary(
            "Linux/Cpp/Zenoh/Lifecycle/Workspace — answered by workspace_features_e2e; this one asserts the rclcpp wrapper",
        ),
    ),
    ("cross_libc_precedence_gate", NonCell::CompileCheck),
    (
        "custom_msg",
        NonCell::Unmodeled(
            "Linux/Rust/Zenoh/CustomMsg/Example — no Example cell (the Workspace Rust row is BuildOnly)",
        ),
    ),
    (
        "custom_transport_loopback",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example under Config::CustomTransport — a feature lane until W4 gives it a configured cell",
        ),
    ),
    (
        "cyclone_slirp_pair",
        NonCell::Unmodeled(
            "two host processes over slirp to stock ros2 — the W2 recipe test, interop-shaped with no ic row",
        ),
    ),
    ("cyclonedds_descriptors", NonCell::Static),
    ("diagnostic_verbatim", NonCell::CompileCheck),
    ("dispatch_strategy", NonCell::Static),
    ("entity_wake", NonCell::FeatureLane),
    ("entry_build", NonCell::CompileCheck),
    (
        "error_handling",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example — answered by linux_example_pubsub_e2e; this one asserts error paths",
        ),
    ),
    (
        "esp32_emulator",
        NonCell::Unmodeled("ESP32 is dormant (issue 1525): every Esp32Qemu cell is a CarveOut"),
    ),
    ("example_output_conformance", NonCell::Static),
    ("example_portability", NonCell::Static),
    ("example_shape", NonCell::Static),
    ("examples_fixture_coverage", NonCell::Static),
    ("exec_depend_drift_check", NonCell::Static),
    ("exec_model_matrix", NonCell::Static),
    (
        "executor",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example — answered by linux_example_pubsub_e2e; executor API tests",
        ),
    ),
    ("executor_arena_advisory", NonCell::FeatureLane),
    (
        "executor_sizing_e2e",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/EntryPubsub/Workspace — answered by deployed_native_system_e2e; this one asserts sizing",
        ),
    ),
    ("fixture_group_resolution", NonCell::Static),
    ("fixture_source_coverage", NonCell::Static),
    ("freertos_firmware_entry", NonCell::CompileCheck),
    (
        "freertos_qemu",
        NonCell::Unmodeled(
            "mps3-an536 Cyclone entry: the board is not a matrix axis value, and the FreertosMps2 Cpp Cyclone row is BuildOnly (S32Z270)",
        ),
    ),
    (
        "freertos_run_plan_runtime",
        NonCell::Secondary(
            "FreertosMps2/Rust/Zenoh/Pubsub/Example (same talker image) — answered by rtos_e2e",
        ),
    ),
    ("ftti_detection_bounds", NonCell::Static),
    (
        "fvp_runtime_ws",
        NonCell::Unmodeled(
            "Fvp/Cpp/Cyclonedds/RealtimeTiers/Workspace — no cell; FVP runs only in verify-fvp-runtime",
        ),
    ),
    (
        "fvp_smoke",
        NonCell::Unmodeled("the FVP board-import surface — no cell; runs in verify-fvp-runtime"),
    ),
    ("generated_message_code_compiles", NonCell::CompileCheck),
    (
        "image_config_workspace",
        NonCell::Unmodeled("a per-image config knob probe — not a matrix workload"),
    ),
    (
        "image_env_cmake_knob",
        NonCell::Unmodeled("an image-env knob probe — not a matrix workload"),
    ),
    ("init_api", NonCell::Static),
    ("integration_nuttx", NonCell::Static),
    ("integration_px4", NonCell::CompileCheck),
    ("interface_subset_linkage", NonCell::CompileCheck),
    ("lane_build_covers_run", NonCell::Static),
    ("lane_run_narrowing", NonCell::Static),
    (
        "large_msg",
        NonCell::Secondary(
            "Linux and QemuBaremetal Rust Pubsub — answered by linux_example_pubsub_e2e / emulator; this one asserts large payloads",
        ),
    ),
    ("launch_py_resolves_as_shipped", NonCell::Static),
    ("launch_synth", NonCell::Static),
    ("legacy_files_forbidden", NonCell::Static),
    (
        "linux_async_roundtrip_e2e",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Service+Action/Example — answered by linux_example_reqresp_e2e; the async API",
        ),
    ),
    (
        "linux_entry_boot",
        NonCell::Secondary(
            "Linux/Rust/EntryPubsub/Workspace boot — answered by deployed_native_system_e2e",
        ),
    ),
    ("linux_main_macro_forms", NonCell::CompileCheck),
    ("linux_main_macro_misuse", NonCell::CompileCheck),
    (
        "linux_orchestration_tiers",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/RealtimeTiers/Workspace — answered by realtime_tiers_e2e",
        ),
    ),
    ("loc_budgets", NonCell::Static),
    (
        "log_arena_knob",
        NonCell::Unmodeled("a log-arena knob probe — not a matrix workload"),
    ),
    ("logging", NonCell::Static),
    (
        "logging_smoke",
        NonCell::Unmodeled("Logging/Example on many platforms — no Example Logging cells exist"),
    ),
    ("macro_one_dep_resolves", NonCell::CompileCheck),
    ("matrix_fixture_coverage", NonCell::Static),
    ("mode_variants", NonCell::Static),
    (
        "multi_node",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example — answered by linux_example_pubsub_e2e; several nodes in one process",
        ),
    ),
    ("multihost_partition_bake", NonCell::Static),
    (
        "nano2nano",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub+Service+Action/Example — answered by the linux_example_* consumers",
        ),
    ),
    ("nav2_compat", NonCell::CompileCheck),
    ("negative_diagnostic_registry", NonCell::Static),
    ("no_local_axis_tables", NonCell::Static),
    ("non_goals_grep", NonCell::Static),
    (
        "nvidia_ivc_mock_wire_format",
        NonCell::Unmodeled("the IVC mock link transport, no RMW — not a matrix workload"),
    ),
    ("on_violation_lowering", NonCell::Static),
    ("orchestration_composable", NonCell::Static),
    ("orchestration_conditionals", NonCell::Static),
    ("orchestration_e2e", NonCell::Static),
    ("orchestration_includes", NonCell::Static),
    ("orchestration_set_remap_env", NonCell::Static),
    (
        "orchestration_tiers_freertos",
        NonCell::Secondary(
            "FreertosMps2/Rust/Zenoh/RealtimeTiers/Workspace — answered by realtime_tiers_e2e",
        ),
    ),
    ("output_marker_gate", NonCell::Static),
    (
        "parameters_roundtrip",
        NonCell::Unmodeled("Linux/C+Cpp/Zenoh/Params/Example — no cell models it"),
    ),
    ("pkg_index", NonCell::CompileCheck),
    ("platform", NonCell::Static),
    ("platform_header_compile", NonCell::CompileCheck),
    (
        "pool_exhaustion_threadx_linux",
        NonCell::Unmodeled("session-pool exhaustion on ThreadX-Linux — not a matrix workload"),
    ),
    ("px4_bridge_compile", NonCell::CompileCheck),
    (
        "px4_xrce",
        NonCell::Unmodeled("px4_msgs over XRCE via px4-stub — Px4 is modelled only as uORB"),
    ),
    ("qemu_patched_binary", NonCell::Static),
    (
        "qos",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example — answered by linux_example_pubsub_e2e; QoS API",
        ),
    ),
    (
        "qos_overrides_runtime_delivery",
        NonCell::Unmodeled("Linux/Rust/Zenoh/Qos/Example — no cell models it"),
    ),
    ("rclrs_talker_port", NonCell::FeatureLane),
    (
        "realtime_subnode_cpp_e2e",
        NonCell::Secondary(
            "Linux/Cpp/Zenoh/RealtimeTiers/Workspace — answered by realtime_tiers_e2e",
        ),
    ),
    (
        "realtime_subnode_cpp_portable_e2e",
        NonCell::Secondary(
            "Linux/Cpp/Zenoh/RealtimeTiers/Workspace — answered by realtime_tiers_e2e",
        ),
    ),
    ("repo_root_is_unified", NonCell::Static),
    (
        "rmw_coordinate_truth",
        NonCell::Secondary(
            "boots every workspace row to check node registration on the right RMW — registration, not delivery, so it answers no cell",
        ),
    ),
    ("ros_editions_bridge", NonCell::Edition),
    ("ros_editions_codegen", NonCell::Edition),
    ("ros_editions_e2e", NonCell::Edition),
    ("ros_editions_nano_interop", NonCell::Edition),
    ("ros_editions_smoke", NonCell::Edition),
    (
        "safety_e2e",
        NonCell::Unmodeled(
            "Linux/Rust+C+Cpp/Zenoh/Safety/Example — no cell models it (the Workspace rows are workspace_features_e2e)",
        ),
    ),
    (
        "sched_dims_applied_e2e",
        NonCell::Secondary(
            "the matrix::SCHED_CELLS consumer (RFC-0051 §6a); its RealtimeTiers cells are answered by realtime_tiers_e2e",
        ),
    ),
    ("sched_dims_model_coverage", NonCell::Static),
    ("schema_serializer_round_trip", NonCell::Static),
    ("self_bringup", NonCell::Static),
    ("serialized_size_bound", NonCell::Static),
    (
        "service_roundtrip_inprocess_e2e",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Service/Workspace — answered by roundtrip_xprocess_e2e; in-process variant",
        ),
    ),
    (
        "services",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Service/Example — answered by linux_example_reqresp_e2e",
        ),
    ),
    ("signal_fd_wake", NonCell::FeatureLane),
    (
        "sim_time_clock_e2e",
        NonCell::Unmodeled("sim-time /clock — not a matrix workload"),
    ),
    ("static_memory_declared_pools", NonCell::CompileCheck),
    ("staticlib_duplicate_symbols", NonCell::CompileCheck),
    ("subnode_freestanding_link", NonCell::CompileCheck),
    ("system_toml_tiers_parse", NonCell::Static),
    ("tier_filter", NonCell::FeatureLane),
    ("time_to_ros_msg", NonCell::Static),
    (
        "trigger_conditions",
        NonCell::Unmodeled("in-process guard/trigger conditions — not a matrix workload"),
    ),
    (
        "violation_channel_e2e",
        NonCell::Unmodeled("the violation channel on Zephyr C++ — not a matrix workload"),
    ),
    ("wake_latency", NonCell::FeatureLane),
    ("weak_symbol_audit", NonCell::Static),
    ("workspace_dirwalk_discovery", NonCell::Static),
    ("workspace_lints_check", NonCell::Static),
    (
        "workspace_metadata",
        NonCell::Secondary(
            "Linux workspace EntryPubsub prebuilt/census checks — the Rust delivery is answered by deployed_native_system_e2e",
        ),
    ),
    ("workspace_shadowing", NonCell::CompileCheck),
    (
        "xrce",
        NonCell::Secondary(
            "Linux/Rust/Xrce/Pubsub/Example (serial too) — answered by linux_example_pubsub_e2e",
        ),
    ),
    ("xrce_concurrent_entities", NonCell::FeatureLane),
    ("xrce_guard_wake", NonCell::FeatureLane),
    ("zenoh_archive_symbols", NonCell::CompileCheck),
    ("zenoh_header_parity", NonCell::CompileCheck),
    ("zephyr_leaf_staleness", NonCell::Static),
    ("zephyr_prjconf_requirements", NonCell::Static),
    ("zephyr_self_pkg", NonCell::CompileCheck),
    (
        "zero_copy",
        NonCell::Secondary(
            "Linux/Rust/Zenoh/Pubsub/Example — answered by linux_example_pubsub_e2e; zero-copy receive",
        ),
    ),
    ("zpico_build_matrix", NonCell::CompileCheck),
];

/// Which consumer answers this cell. THE assignment — disjoint by construction
/// (the first matching arm wins, and the arms are ordered so none shadows a
/// narrower one). Total over Runtime cells; `None` only for a BuildOnly or
/// CarveOut cell nobody runs.
pub fn consumer_of(cell: &Cell) -> Option<Consumer> {
    use Consumer::*;
    use Kind as K;
    use Lang as L;
    use PlatformId as P;
    use Rmw as R;
    use Workload as W;
    // A CONFIGURED cell is answered by the test of its configuration
    // (RFC-0051 §7a): its default-build sibling's consumer runs the default
    // build, which is exactly what the configuration is not.
    if !cell.config.is_empty() {
        return match (cell.platform, cell.config) {
            (P::Linux, [Config::Lending, Config::Multisession]) => Some(LoanE2e),
            _ => None,
        };
    }
    match (cell.platform, cell.lang, cell.rmw, cell.workload, cell.kind) {
        // ── the phase-329 W1 consumers, which derive their cases ──────────
        (_, _, _, W::Multihost, _) => Some(Multihost),
        (_, _, _, W::RealtimeTiers, _) => Some(RealtimeTiers),
        (_, _, _, W::Service | W::Action, K::Workspace) => Some(Roundtrip),
        // entry_e2e — the RTOS EntryPubsub subset it boots. ZENOH only: its
        // exec table is keyed without the RMW, so a Cyclone cell here booted
        // the zenoh image (phase-510 W3 found it on Zephyr C++).
        (P::ThreadxLinux, L::C | L::Cpp | L::Mixed, R::Zenoh, W::EntryPubsub, _) => Some(Entry),
        (P::ThreadxRiscv64, L::C, R::Zenoh, W::EntryPubsub, _) => Some(Entry),
        (P::FreertosMps2, L::C | L::Cpp | L::Mixed, R::Zenoh, W::EntryPubsub, _) => Some(Entry),
        (P::ZephyrNativeSim, L::C | L::Cpp | L::Mixed, R::Zenoh, W::EntryPubsub, _) => Some(Entry),
        (P::NuttxArm, L::C | L::Rust, R::Zenoh, W::EntryPubsub, _) => Some(Entry),
        (
            P::ZephyrNativeSim,
            L::Rust,
            _,
            W::Params | W::Qos | W::Lifecycle | W::Safety,
            K::Workspace,
        ) => Some(Entry),
        (P::ZephyrNativeSim, L::Cpp, _, W::Params, K::Workspace) => Some(Entry),
        (
            P::Linux,
            _,
            _,
            W::CustomMsg | W::Logging | W::Qos | W::Lifecycle | W::Safety | W::Remap,
            K::Workspace,
        ) => Some(WorkspaceFeatures),
        // ── consumers that hand-list their cases (`assert_consumer_cases`) ─
        (P::Linux, _, _, W::Pubsub, K::Example) => Some(LinuxExamplePubsub),
        (P::Linux, _, _, W::Service | W::Action, K::Example) => Some(LinuxExampleReqresp),
        (P::Linux, _, _, W::ExecutorBoundNode, K::Example) => Some(LinuxExampleExecutorBoundNode),
        (_, _, _, W::Port | W::PortSmoke | W::PortMonitor, K::Example) => Some(PortTemplates),
        (P::ZephyrQemuCortexM, _, _, W::Pubsub | W::BootReport, K::Example) => Some(ZephyrCortexM),
        (P::ZephyrNativeSim, _, _, W::Pubsub | W::Service | W::Action, K::Example) => Some(Zephyr),
        (P::ZephyrNativeSim, L::Rust, R::Zenoh, W::EntryPubsub, K::Workspace) => Some(Zephyr),
        (
            P::FreertosMps2 | P::NuttxArm | P::ThreadxLinux | P::ThreadxRiscv64,
            _,
            R::Zenoh,
            W::Pubsub | W::Service | W::Action,
            K::Example,
        ) => Some(RtosE2e),
        (_, _, _, W::TypedCarrier, K::Example) => Some(TypedCarrier),
        (_, _, _, W::Params, K::Example) => Some(ParamStoreCarve),
        (P::ThreadxLinux, _, R::Cyclonedds, _, K::Example) => Some(LinuxApi),
        (P::ThreadxRiscv64, _, _, W::Errno, K::Example) => Some(ThreadxRiscv64Qemu),
        (P::ThreadxRiscv64, _, R::Cyclonedds, W::Pubsub, K::Example) => Some(ThreadxRiscv64Qemu),
        (P::NuttxRiscv, _, _, W::Pubsub, K::Example) => Some(CRiscvNuttx),
        (P::QemuBaremetal, _, _, _, K::Example) => Some(Emulator),
        (P::Linux, L::Rust, R::Zenoh, W::EntryPubsub, K::Workspace) => Some(DeployedNativeSystem),
        (P::FreertosMps2, L::Rust, R::Zenoh, W::EntryPubsub, K::Workspace) => Some(BridgeNetwork),
        (P::FreertosPosix, _, _, W::EntryPubsub, K::Workspace) => Some(FreertosPosix),
        (P::Linux, L::C | L::Cpp, _, W::Params, K::Workspace) => Some(CppCParamLiveRead),
        (P::Linux, L::Rust, _, W::Params, K::Workspace) => Some(ParamLiveRead),
        _ => None,
    }
}

/// One cell's coordinate, as a consumer declares the cases it runs.
pub type CellCoord = (PlatformId, Lang, Rmw, Workload, Kind);

fn cell_key(coord: CellCoord, config: &[Config]) -> String {
    if config.is_empty() {
        format!("{coord:?}")
    } else {
        format!("{coord:?} +{config:?}")
    }
}

/// The Runtime cells [`consumer_of`] assigns to `consumer`, as coordinates
/// (with the configuration set, for a configured cell).
pub fn consumer_cells(consumer: Consumer) -> std::collections::BTreeSet<String> {
    runtime_cells()
        .filter(|c| consumer_of(c) == Some(consumer))
        .map(|c| cell_key((c.platform, c.lang, c.rmw, c.workload, c.kind), c.config))
        .collect()
}

/// The binding a hand-listing consumer carries (RFC-0051 §7c): the cases it
/// runs are exactly the Runtime cells assigned to it. A case with no cell
/// runs a coordinate the matrix does not model; a cell with no case is a
/// Runtime cell nobody runs. Both are reds here, with the coordinate named.
pub fn assert_consumer_cases(consumer: Consumer, covered: &[CellCoord]) {
    let configured: Vec<(CellCoord, &'static [Config])> =
        covered.iter().map(|c| (*c, &[][..])).collect();
    assert_consumer_config_cases(consumer, &configured);
}

/// [`assert_consumer_cases`] for a consumer that runs CONFIGURED cells: each
/// case names its configuration set too (RFC-0051 §7a).
pub fn assert_consumer_config_cases(
    consumer: Consumer,
    covered: &[(CellCoord, &'static [Config])],
) {
    let declared: std::collections::BTreeSet<String> =
        covered.iter().map(|(c, k)| cell_key(*c, k)).collect();
    let assigned = consumer_cells(consumer);
    let unrun: Vec<_> = assigned.difference(&declared).collect();
    let unmodeled: Vec<_> = declared.difference(&assigned).collect();
    assert!(
        unrun.is_empty() && unmodeled.is_empty(),
        "`tests/{}.rs` ({consumer:?}) is out of step with matrix::CELLS:\n  \
         Runtime cells assigned to it that it does not run: {unrun:?}\n  \
         cases it runs that no Runtime cell assigned to it models: {unmodeled:?}\n\
         Fix the case list, the cell's tier, or `consumer_of` (RFC-0051 §7c).",
        consumer.binary()
    );
}

// =============================================================================
// Realtime-dim matrix (RFC-0051 §6a, phase-329 W2)
// =============================================================================

/// The scheduler knob whose HONORING (or honest fallback) a sched-dim cell
/// asserts at boot. Each is a real-time tier attribute the `ws-realtime`
/// fixtures declare in `system_model.yaml`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SchedDim {
    /// `core_id` pin → `k_thread_cpu_pin` / `pthread_setaffinity` / … .
    CorePin,
    /// `deadline_us` → `k_thread_deadline_set` (Zephyr EDF).
    EdfDeadline,
    /// `preempt_threshold` → ThreadX `tx_thread_preemption_change`.
    PreemptThreshold,
    /// `time_slice_us` → ThreadX `tx_thread_time_slice_change`.
    TimeSlice,
    /// `budget_us`/`period_us` → NuttX `SCHED_SPORADIC`.
    SporadicBudget,
    /// tier priority → NuttX per-tier `SCHED_FIFO` priority.
    TierPriority,
    /// issue 0260 — `core` verified as PLACEMENT on a MULTI-CORE image: the
    /// tier is observed running on the cpu it declared.
    ///
    /// Distinct from [`SchedDim::CorePin`], which verifies that the kernel
    /// ACCEPTED the pin. On a uniprocessor image acceptance is true and says
    /// nothing — cpu 0 is the only cpu the tier could be on — so the two dims
    /// assert genuinely different claims and both are worth keeping.
    CorePinPlacement,
    /// issue 1537 — a DERIVED tier priority lands BELOW the image's own
    /// transport band: the Zephyr boot report
    /// (`nros_zephyr_report_tier_vs_transport`) stays silent on an image whose
    /// band differs from the Kconfig defaults projection.
    ///
    /// Distinct from [`SchedDim::TierPriority`], which asserts an AUTHORED
    /// priority is adopted. Here nothing is authored: `nros::main!` derives the
    /// table, and the claim is about WHERE it allocated — out of the image's
    /// `.config`, or out of the projection. Only a lowered-band image can tell
    /// those apart; on a default-band image they are the same numbers.
    DerivedTierBelowTransport,
}

/// One realtime-dim matrix cell: a `(dim × platform × lang)` coordinate. Like
/// [`Cell`] this is NEUTRAL — the boot mechanism, markers, and assert shape are
/// the consumer's execution data (`tests/sched_dims_applied_e2e.rs`), not table
/// facts. Every cell boots a `ws-realtime-*` fixture (shared with
/// `realtime_tiers_e2e`).
#[derive(Copy, Clone, Debug)]
pub struct SchedCell {
    pub dim: SchedDim,
    pub platform: PlatformId,
    pub lang: Lang,
    pub tier: Tier,
}

const fn sched(dim: SchedDim, platform: PlatformId, lang: Lang, tier: Tier) -> SchedCell {
    SchedCell {
        dim,
        platform,
        lang,
        tier,
    }
}

/// The realtime-dim matrix (phase-329 W2 — replaces the 10 hand-written
/// `*_applied.rs` files, one `#[test]` each). Every dim a `ws-realtime` fixture
/// declares has a cell here; the consumer iterates them.
pub const SCHED_CELLS: &[SchedCell] = {
    use Lang::*;
    use PlatformId::*;
    use SchedDim::*;
    use Tier::Runtime;
    &[
        // core-pin — one per platform family that can pin (RFC-0052 fail-loud).
        sched(CorePin, ZephyrNativeSim, Rust, Runtime),
        sched(CorePin, NuttxArm, Rust, Runtime),
        sched(CorePin, ThreadxLinux, Rust, Runtime),
        sched(CorePin, FreertosMps2, Cpp, Runtime),
        sched(CorePin, Linux, Rust, Runtime),
        // issue 0260 — the ONLY multi-core cell: `core = 1` observed, not just
        // accepted. Zephyr/C on `qemu_cortex_a53/.../smp`.
        sched(CorePinPlacement, ZephyrNativeSim, C, Runtime),
        // Zephyr EDF deadline — all three language arms (W5.5 + W5.8).
        sched(EdfDeadline, ZephyrNativeSim, Rust, Runtime),
        sched(EdfDeadline, ZephyrNativeSim, Cpp, Runtime),
        sched(EdfDeadline, ZephyrNativeSim, C, Runtime),
        // NuttX sporadic + tier-priority.
        sched(SporadicBudget, NuttxArm, Cpp, Runtime),
        sched(TierPriority, NuttxArm, Rust, Runtime),
        // issue 0636 — the C/C++ arm of the same board. `nuttx_run_tiers.c`
        // took `&tiers[0]` as its session owner long after the Rust arm stopped,
        // and nothing noticed because no cell asserted tier priority on this
        // arm at all: the C arm applied priorities at `pthread_create` time and
        // printed nothing, so #579's "every declaring tier prints its marker"
        // was enforced in one language only. Both arms now route through
        // `nros_nuttx_apply_current_priority`, one implementation and one
        // marker, and this cell is what keeps them from diverging again.
        sched(TierPriority, NuttxArm, Cpp, Runtime),
        // issue 0636 — the FreeRTOS arm of the same rule. Added with the seam
        // that made it assertable: this kernel printed no tier-priority marker
        // in either language, so #579 was enforced on NuttX alone and a boot
        // task adopting NO priority at all had no cell that could see it. Both
        // languages, because one seam TU serves both entries.
        sched(TierPriority, FreertosMps2, Cpp, Runtime),
        sched(TierPriority, FreertosMps2, C, Runtime),
        // issue 1537 — the ONLY Rust Zephyr image whose tiers `nros::main!`
        // DERIVES (`realtime-rust/src/derived_bringup`), on an image with a
        // lowered transport band. A macro that regressed to the Kconfig
        // defaults projection puts both tiers inside that band and the boot
        // report fires; measured, not reasoned.
        sched(DerivedTierBelowTransport, ZephyrNativeSim, Rust, Runtime),
        // ThreadX preempt-threshold + time-slice.
        sched(PreemptThreshold, ThreadxLinux, Rust, Runtime),
        sched(TimeSlice, ThreadxLinux, Rust, Runtime),
    ]
};

/// Runtime sched-dim cells — what the consumer iterates.
pub fn sched_runtime_cells() -> impl Iterator<Item = &'static SchedCell> {
    SCHED_CELLS
        .iter()
        .filter(|c| matches!(c.tier, Tier::Runtime))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-329 W1 gate G5 — the consumer-claim partition is sound.
    ///
    /// (1) Every Runtime cell of a FULLY-OWNED workload (`Multihost`,
    /// `RealtimeTiers`, workspace `Service`/`Action`) is claimed by a W1
    /// consumer — so adding such a cell to `CELLS` can never leave it un-run
    /// (the "adding a row adds no test" defect). (2) Every consumer still claims
    /// at least one Runtime cell — a tripwire against a workload/platform set
    /// silently vanishing from `CELLS`. Exec-arm totality over each claim is
    /// enforced at runtime (a claimed cell with no `exec_for` arm panics loudly
    /// in the consumer), since exec tables live in the per-test binaries.
    #[test]
    fn every_runtime_cell_has_a_consumer() {
        // RFC-0051 §7c — `consumer_of` is TOTAL over Runtime cells.
        let unowned: Vec<_> = runtime_cells()
            .filter(|c| consumer_of(c).is_none())
            .collect();
        assert!(
            unowned.is_empty(),
            "Runtime cell(s) no consumer answers — give each a `consumer_of` arm and a \
             test that runs it, or a tier saying what unlocks it: {unowned:#?}"
        );
        // And every consumer still owns a cell — a tripwire against a
        // consumer file whose workload/platform set vanished, so it runs nothing.
        for &cons in Consumer::ALL {
            assert!(
                runtime_cells().any(|c| consumer_of(c) == Some(cons)),
                "consumer {cons:?} (`tests/{}.rs`) claims no Runtime cell",
                cons.binary()
            );
        }
    }

    /// phase-329 W2 — the realtime-dim table is populated and every declared
    /// `SchedDim` variant has at least one cell (a tripwire against a dim's
    /// cells vanishing from the table, the 0380 loss shape at the matrix layer).
    #[test]
    fn sched_dims_table_covers_every_dim() {
        assert!(!SCHED_CELLS.is_empty(), "the realtime-dim matrix is empty");
        for dim in [
            SchedDim::CorePin,
            SchedDim::EdfDeadline,
            SchedDim::PreemptThreshold,
            SchedDim::TimeSlice,
            SchedDim::SporadicBudget,
            SchedDim::TierPriority,
            SchedDim::CorePinPlacement,
            SchedDim::DerivedTierBelowTransport,
        ] {
            assert!(
                sched_runtime_cells().any(|c| c.dim == dim),
                "realtime-dim {dim:?} has no Runtime cell — its coverage vanished from SCHED_CELLS"
            );
        }
    }

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
                // phase-510 — a configured cell shares its default-build
                // sibling's coordinate; the configuration set tells them apart.
                c.config.iter().map(|k| *k as u8).collect::<Vec<_>>(),
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

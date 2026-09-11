//! Issue 0352 / phase-324 — THE interop & bridge test-intent list.
//!
//! [`crate::matrix::CELLS`] enumerates baked, self-contained cells. Interop and
//! bridge cells are a different shape: a nano side that is BUILT, plus an
//! ephemeral PEER (a stock ROS 2 node, an XRCE Agent, another nano bridge) and a
//! DIRECTION. `Cell` cannot carry a peer or a direction, so these cells live
//! here in the formulation their shape needs — an [`InteropCell`] wrapping the
//! nano [`Cell`] with `build` / `peer` / `dir` / `test`.
//!
//! The correspondence between what is TESTED (this list + `matrix::CELLS`), what
//! is BUILT (each cell's [`BuildChannel`], recipe named not invoked — the three
//! channels build DIFFERENTLY on purpose, issue 0352 non-goal: no unifier) and
//! what RUNS (each cell's `test`) is one [`Binding`] per cell, gated in
//! `tests/matrix_fixture_coverage.rs` (G1 coverage, G2 build-coord match, G3
//! tier, G4 peer-decl). A cell whose declared `(lang, rmw)` disagrees with the
//! fixture its test builds — the issue 0341 defect-2 drift class — is a gate
//! failure, not a silent pass.
//!
//! NOT modelled here: the docker per-edition harness (`ros_editions_e2e.rs`).
//! That is the ROS-edition axis (a per-run global, issue 0327), not a matrix
//! cell — it has no baked nano fixture in this list.

use crate::matrix::{Cell, Kind, PlatformId, Rmw, TestCell, Tier};

/// Which build channel produces an interop/bridge cell's NANO side.
///
/// The channels build differently on purpose; a channel only declares which
/// platform it can produce, so G2 can reject a cell pointed at a channel that
/// cannot build its coordinate.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BuildChannel {
    /// Native example / workspace-entry binaries (`just native build-fixtures`
    /// families). Host platform only.
    NativeFixtures,
    /// Zephyr workspace entry or example leaf via the west leaves lane
    /// (`scripts/build/zephyr-fixture-leaves.sh`, driven by `just zephyr
    /// build-fixtures`).
    ///
    /// One channel, two BOARDS. The lane emits `native_sim/native/64` leaves and
    /// `mps2_an385` leaves from the same manifest loop, so both
    /// `PlatformId::ZephyrNativeSim` and `PlatformId::ZephyrQemuCortexM` are
    /// producible here (phase-441 W1) — the board is a row field, not a channel.
    ZephyrWestLeaves,
    /// FreeRTOS workspace entry cross-built for MPS2-AN385 (Cortex-M3) by
    /// `just freertos build-fixtures` — `scripts/build/workspace-fixtures-build.sh
    /// freertos <lang>`, driven off the `platform = "freertos"` rows of
    /// `examples/fixtures.toml`. The artifact is an ELF QEMU boots; the guest's
    /// lwIP stack dials the baked `tcp/192.0.3.1:<port>` locator through the
    /// slirp gateway (phase-441 W3).
    FreertosMps2Fixtures,
}

impl BuildChannel {
    /// The `just` recipe that builds this channel's artifacts — NAMED, not
    /// invoked. Gated against the justfile by G2 the way `PlatformId::just_module`
    /// is by `just_module_names_a_real_module`.
    pub const fn build_recipe(self) -> &'static str {
        match self {
            BuildChannel::NativeFixtures => "just native build-fixtures",
            BuildChannel::ZephyrWestLeaves => "just zephyr build-fixtures",
            BuildChannel::FreertosMps2Fixtures => "just freertos build-fixtures",
        }
    }

    /// The `just` module the recipe lives under (what G2 checks exists).
    pub const fn just_module(self) -> &'static str {
        match self {
            BuildChannel::NativeFixtures => "native",
            BuildChannel::ZephyrWestLeaves => "zephyr",
            BuildChannel::FreertosMps2Fixtures => "freertos",
        }
    }

    /// Can this channel build the given platform? The G2 coord check: a cell
    /// whose platform this channel cannot produce is a mis-declared binding.
    pub const fn builds_platform(self, p: PlatformId) -> bool {
        match self {
            BuildChannel::NativeFixtures => matches!(p, PlatformId::Linux),
            BuildChannel::ZephyrWestLeaves => matches!(
                p,
                PlatformId::ZephyrNativeSim | PlatformId::ZephyrQemuCortexM
            ),
            BuildChannel::FreertosMps2Fixtures => matches!(p, PlatformId::FreertosMps2),
        }
    }
}

/// The ephemeral peer a cell runs against. DECLARED, never built. A bridge names
/// BOTH endpoints.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Peer {
    /// A stock ROS 2 node of the run's edition (`NROS_ROS_EDITION`), speaking
    /// `rmw` (`rmw_zenoh_cpp` / `rmw_cyclonedds_cpp`).
    RosEdition(Rmw),
    /// nano XRCE client → micro-XRCE-DDS Agent → `rmw_fastrtps_cpp`.
    XrceAgent,
    /// nano declarative bridge: `ingress` rmw in → `egress` rmw out, then a ROS 2
    /// peer on the egress side.
    NanoBridge { ingress: Rmw, egress: Rmw },
}

impl Peer {
    /// True if the peer is internally consistent for `cell` (G4). For a
    /// single-RMW peer the rmw matches the cell; a bridge's `ingress` matches the
    /// cell's declared rmw (the nano side dials the ingress).
    pub fn consistent_with(self, cell: &Cell) -> bool {
        match self {
            Peer::RosEdition(rmw) => rmw == cell.rmw,
            // The XRCE Agent bridges nano-XRCE ⇄ fastrtps; the cell's rmw is Xrce.
            Peer::XrceAgent => matches!(cell.rmw, Rmw::Xrce),
            // A bridge cell's rmw is the INGRESS the nano side speaks.
            Peer::NanoBridge { ingress, egress } => ingress == cell.rmw && ingress != egress,
        }
    }
}

/// Which way data flows across the interop boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dir {
    /// nano-ros node → ROS 2 peer (nano is source / client).
    NanoToRos,
    /// ROS 2 peer → nano-ros node (nano is sink / server).
    RosToNano,
    /// Both directions in one test.
    BiDir,
}

/// One interop/bridge cell: the built nano side plus its peer, direction, build
/// channel and the test that runs it.
#[derive(Copy, Clone, Debug)]
pub struct InteropCell {
    /// Stable name, e.g. `"zephyr-qos-rust-zenoh"`. The `Binding` key.
    pub id: &'static str,
    /// The nano side. `cell.kind` is [`Kind::Interop`] or [`Kind::Bridge`];
    /// `cell.platform/lang/rmw/workload/tier` describe the built artifact.
    pub cell: Cell,
    /// How the nano side is built.
    pub build: BuildChannel,
    /// The ephemeral peer it runs against.
    pub peer: Peer,
    /// Data-flow direction.
    pub dir: Dir,
    /// The test binary (`cargo test --test <name>`) that runs this cell. A
    /// [`Tier::CarveOut`] cell that nothing runs carries [`NO_TEST`].
    pub test: &'static str,
}

/// `test` sentinel for a carved-out cell no test runs.
pub const NO_TEST: &str = "(carved-out — no runtime lane)";

impl TestCell for InteropCell {
    fn cell(&self) -> &Cell {
        &self.cell
    }
}

/// The correspondence row for one runnable test: which cell, built by which
/// recipe, run by which test. The row NAMES the recipes — it does not build or
/// run. This is the issue-0352 SSoT that ties BUILD and TEST together without
/// unifying either.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub cell_id: &'static str,
    pub build_recipe: &'static str,
    pub test_recipe: &'static str,
}

impl InteropCell {
    /// The binding row for this cell.
    pub fn binding(&self) -> Binding {
        Binding {
            cell_id: self.id,
            build_recipe: self.build.build_recipe(),
            test_recipe: self.test,
        }
    }
}

const fn ic(
    id: &'static str,
    cell: Cell,
    build: BuildChannel,
    peer: Peer,
    dir: Dir,
    test: &'static str,
) -> InteropCell {
    InteropCell {
        id,
        cell,
        build,
        peer,
        dir,
        test,
    }
}

// Shorthand for the seed table.
use crate::matrix::{Lang::*, PlatformId::*, Rmw::*, Tier::*, Workload::*};
use BuildChannel::*;
use Dir::*;
use Kind::{Bridge, Interop};
use Peer::*;

/// Build a nano `Cell` for an interop/bridge row inline — the interop list is
/// the SSoT for these coordinates now, so it constructs its own cells rather
/// than referencing rows removed from `matrix::CELLS`.
const fn c(
    platform: PlatformId,
    lang: crate::matrix::Lang,
    rmw: Rmw,
    workload: crate::matrix::Workload,
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

/// THE interop & bridge cells (issue 0352 / phase-324). Moved out of
/// `matrix::CELLS` verbatim (the 6 native ROS-2 interop cells, the zephyr QoS
/// interop pair, the native lifecycle interop cell, the 2 declarative bridge
/// cells), now carrying their peer / direction / build channel / test.
#[rustfmt::skip]
pub const CELLS: &[InteropCell] = &[
    // ── Native nano ↔ stock ROS 2 (host), zenoh + cyclone ───────────────
    // tests/interop_e2e.rs — nano example bins vs `ros2 topic`/`ros2 service`.
    ic("native-pubsub-rust-zenoh-n2r",
       c(Linux, Rust, Zenoh, Pubsub, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), NanoToRos, "interop_e2e"),
    ic("native-service-rust-zenoh-r2n",
       c(Linux, Rust, Zenoh, Service, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), BiDir, "interop_e2e"),
    // phase-433 W3 — the cyclone half is C, not Rust. `interop_e2e`'s three
    // cyclone cases spawn `nano_cyclone_c_binary(...)`: `c_talker`,
    // `c_listener`, `c_service_server` out of `examples/native/c/`. These rows
    // said Rust, `scenario_coord` returned `Lang::Rust` unconditionally, and
    // the per-case tripwire compared the two — so the language axis was
    // inverted and agreed with itself. The vacated Rust/Cyclonedds shapes are
    // carved below.
    ic("native-pubsub-c-cyclone-n2r",
       c(Linux, C, Cyclonedds, Pubsub, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), NanoToRos, "interop_e2e"),
    ic("native-service-c-cyclone-r2n",
       c(Linux, C, Cyclonedds, Service, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), BiDir, "interop_e2e"),
    // The shapes the two rows above used to claim. Recorded rather than
    // dropped because the artifacts EXIST — `examples/native/rust/{talker,
    // listener,service-server,service-client}` all have `linux/rust/cyclonedds`
    // rows in `examples/fixtures.toml` — so the absence is a missing lane, not
    // a missing build, and nothing else in the tree would say so. Rust ↔
    // stock-ROS-2 over Cyclone is not wholly unproven: `native-graph-rust-
    // cyclone-r2n` runs that pairing for the Graph workload. Delivery is what
    // has never been run.
    ic("native-pubsub-rust-cyclone-n2r-CARVED",
       c(Linux, Rust, Cyclonedds, Pubsub, Interop,
         CarveOut("no Rust/Cyclonedds pubsub-interop lane; interop_e2e's cyclone \
                   pubsub cases run the C examples (c_talker/c_listener). The Rust \
                   cyclone talker/listener fixtures are built, so the lane is \
                   affordable — file one if wanted.")),
       NativeFixtures, RosEdition(Cyclonedds), NanoToRos, NO_TEST),
    ic("native-service-rust-cyclone-r2n-CARVED",
       c(Linux, Rust, Cyclonedds, Service, Interop,
         CarveOut("no Rust/Cyclonedds service-interop lane; interop_e2e's cyclone \
                   service case runs the C example (c_service_server). The Rust \
                   cyclone service-server/client fixtures are built, so the lane is \
                   affordable — file one if wanted.")),
       NativeFixtures, RosEdition(Cyclonedds), BiDir, NO_TEST),

    // ── phase-381 — READ the graph a stock ROS 2 node is in ──────────────
    // tests/graph_interop.rs. The only cell whose subject is DISCOVERY rather
    // than delivery, and the one that would have caught issue 0903: twelve
    // slots were produced, reachable from three languages, mutation-tested and
    // `check-api-parity`-clean while the feature did not work, because every
    // check tested our code against our own assumptions.
    ic("native-graph-rust-zenoh-r2n",
       c(Linux, Rust, Zenoh, Graph, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), RosToNano, "graph_interop"),
    // Cyclone's half. `graph.cpp` PUBLISHED `ros_discovery_info` since
    // phase-177.36 and only gained a reader in W5, which has never been run
    // against a live participant.
    ic("native-graph-rust-cyclone-r2n",
       c(Linux, Rust, Cyclonedds, Graph, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), RosToNano, "graph_interop"),

    // ── phase-433 W6 — the ACTIONS family's live peer ────────────────────
    // tests/ros2_action_e2e.rs. Until this row the family had NO interop cell
    // at all: every action row in `matrix::CELLS` is nano-to-nano, and both
    // ends of such a pair share whatever convention `service.cpp`'s five CDR
    // adapters implement, so the one property the adapters exist to provide is
    // the one property those rows cannot observe (issue 0976). Actions are also
    // the family with the widest unexplained runtime spread — issue 0902
    // measured goals completing 20–90 % of the time on one build — which is a
    // shape only a live peer surfaces.
    //
    // BOTH directions, one coordinate. The adapters sit on both sides of the
    // service path: `strip_goal_id_len_at` / `strip_nested_cdr_at` fire only
    // when nano-ros WRITES a SendGoal/GetResult request, so the R2N cell (a
    // stock `ros2 action send_goal` into the nano server) does not reach them
    // and the N2R cell (the nano client into a stock server) is where they run.
    // `coords_for` collapses direction, so `assert_test_bound` in that file
    // names the coordinate once.
    ic("native-action-rust-cyclone-r2n",
       c(Linux, Rust, Cyclonedds, Action, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), RosToNano, "ros2_action_e2e"),
    ic("native-action-rust-cyclone-n2r",
       c(Linux, Rust, Cyclonedds, Action, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), NanoToRos, "ros2_action_e2e"),

    // ── Native nano XRCE ↔ Agent ↔ fastrtps ─────────────────────────────
    // tests/xrce_ros2_interop.rs.
    ic("native-pubsub-rust-xrce-n2r",
       c(Linux, Rust, Xrce, Pubsub, Interop, Runtime),
       NativeFixtures, XrceAgent, NanoToRos, "xrce_ros2_interop"),
    ic("native-service-rust-xrce-r2n",
       c(Linux, Rust, Xrce, Service, Interop, Runtime),
       NativeFixtures, XrceAgent, BiDir, "xrce_ros2_interop"),

    // ── Native nano lifecycle ↔ `ros2 lifecycle` ────────────────────────
    ic("native-lifecycle-rust-zenoh",
       c(Linux, Rust, Zenoh, Lifecycle, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), BiDir, "interop_e2e"),

    // ── Zephyr native_sim QoS interop ───────────────────────────────────
    // phase-441 W5 — this header read "Zephyr on-target QoS interop", which
    // claimed what the coordinate does not support: `ZephyrNativeSim` is
    // `native_sim/native/64`, where the sockets are OFFLOADED to the host and
    // the pointer width is the host's, so no RTOS network stack is in the path.
    // The cell is real coverage (it caught issue #141) and it is not a witness
    // for a device. `ZephyrQemuCortexM` is the coordinate that would be.
    // Issue 0341 — the ONLY runtime test of this shape
    // (qos_zephyr_ros2_interop_e2e.rs) boots the RUST `ws-qos-rust` zephyr entry
    // over zenoh-pico → rmw_zenoh_cpp. The matrix used to declare Cpp/Cyclonedds,
    // which nothing ran (defect 2). Model reality here; carve the never-run shape.
    ic("zephyr-qos-rust-zenoh",
       c(ZephyrNativeSim, Rust, Zenoh, Qos, Interop, Runtime),
       ZephyrWestLeaves, RosEdition(Zenoh), BiDir, "qos_zephyr_ros2_interop_e2e"),
    ic("zephyr-qos-cpp-cyclone-CARVED",
       c(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop,
         CarveOut("no zephyr Cpp/Cyclonedds QoS-interop lane; the QoS zephyr \
                   interop test runs Rust/Zenoh (zenoh-pico). File a lane if wanted.")),
       ZephyrWestLeaves, RosEdition(Cyclonedds), BiDir, NO_TEST),

    // ── phase-441 W1 — the live peer, one BOARD over ─────────────────────
    // The row above is `ZephyrNativeSim`, i.e. `native_sim/native/64`:
    // `CONFIG_NET_SOCKETS_OFFLOAD=y`, so its sockets, its libc and its 64-bit
    // pointers are the HOST's and no RTOS network stack is ever in the path.
    // Until this row, that was the whole of our non-Linux live-peer coverage —
    // one platform, one language, one RMW, one workload — and calling it
    // "on-target" was doing work the artifact did not support.
    //
    // This cell is `mps2_an385`: a 32-bit Cortex-M3 running Zephyr's IN-KERNEL
    // IP stack over `eth_smsc911x`, reaching a host router through QEMU SLIRP.
    // Everything else is deliberately held still — same RMW (zenoh-pico), same
    // peer, same direction, same crossing mechanism (a baked TCP locator, not
    // multicast: SLIRP is unicast-only and TAP needs root).
    //
    // Two axes move rather than one, and the second is forced. The workload is
    // `Pubsub`, not the sibling's `Qos`, because no QoS workspace entry exists
    // for any board but native_sim in ANY language — a QoS cell here would mean
    // authoring an entry, a `[[workspace_fixture]]` row, a west build name and
    // a port bake, none of which is the axis W1 exists to move. The C talker
    // leaf is already built by the west lane at exactly this coordinate
    // (`build-cortex-m-c-talker-zenoh`, locator `tcp/10.0.2.2:10700`), so this
    // row adds a live peer and nothing else.
    //
    // The language is C, and NOT for the reason phase-441 gives: issue 0432
    // (`zephyr-lang-rust` cannot build for a board with gpio nodes) was
    // RESOLVED 2026-08-12 by phase-346 W2/W3 and the Rust leaf has run since.
    // C is what the existing lane-built leaf is; it also exercises the other
    // half of our API surface from the Rust sibling above.
    ic("zephyr-cortex-m-pubsub-c-zenoh",
       c(ZephyrQemuCortexM, C, Zenoh, Pubsub, Interop, Runtime),
       ZephyrWestLeaves, RosEdition(Zenoh), NanoToRos,
       "pubsub_zephyr_cortex_m_ros2_interop_e2e"),

    // ── phase-441 W3 — the SECOND KERNEL's live peer ────────────────────
    // tests/pubsub_freertos_ros2_interop_e2e.rs. Every other on-target row in
    // this list is Zephyr native_sim, whose sockets, pointer width and libc are
    // the HOST's (`matrix.rs`: "Zephyr native_sim (NSOS host sockets)"). So
    // before this row the live-peer coverage of the RTOS ports was one kernel,
    // one board, and a network stack that is not an RTOS network stack.
    //
    // This one is a different kernel (FreeRTOS), a different IP stack (lwIP
    // over the emulated LAN9118), a 32-bit target (thumbv7m), a different libc
    // (newlib-nano) and a real emulator — and it reaches the host over QEMU
    // SLIRP, unicast only, which is what makes it affordable: CLAUDE.md forbids
    // `sudo`, TAP needs `ip tuntap add`, and zenoh-pico's baked
    // `tcp/192.0.3.1:<port>` locator needs no multicast at all. (Cyclone's SPDP
    // does; that is issue 1251 and phase-441 W2's measurement, deliberately not
    // this cell's problem.)
    //
    // C rather than Rust, and that is a second axis moved for free: the
    // existing on-target row runs the Rust API, this one runs the C ABI
    // (`nros_cpp_publisher_create` out of `examples/workspaces/c`) — the half
    // of the surface an RTOS consumer is most likely to be using.
    //
    // It reuses `entry_e2e`'s freertos_c image and therefore its baked router
    // port (`port_of(FreertosMps2, C, EntryPubsub)`), exactly as the zephyr QoS
    // interop cell reuses the `ws-qos-rust` image and port. Two binaries on one
    // port must not run at once, so `.config/nextest.toml` puts this one in
    // `matrix-consumers-serial` with `entry_e2e` — and the override has to sit
    // ABOVE `binary(~freertos)`, or `qemu-emulated` claims it first and the
    // serialization silently does not happen (the phase-373 W1 defect, one
    // binary over).
    ic("freertos-mps2-pubsub-c-zenoh-n2r",
       c(FreertosMps2, C, Zenoh, EntryPubsub, Interop, Runtime),
       FreertosMps2Fixtures, RosEdition(Zenoh), NanoToRos,
       "pubsub_freertos_ros2_interop_e2e"),

    // ── Declarative cross-RMW bridges ───────────────────────────────────
    // The nano bridge is a `ws-bridge-*-rust` native_entry; a ROS 2 peer sits on
    // the egress side. cell.rmw = the INGRESS the nano side dials.
    ic("bridge-zenoh-to-cyclone",
       c(Linux, Rust, Zenoh, Pubsub, Bridge, Runtime),
       NativeFixtures, NanoBridge { ingress: Zenoh, egress: Cyclonedds }, NanoToRos,
       "declarative_bridge_zenoh_to_cyclonedds"),
    ic("bridge-zenoh-to-xrce",
       c(Linux, Rust, Zenoh, Pubsub, Bridge, Runtime),
       NativeFixtures, NanoBridge { ingress: Zenoh, egress: Xrce }, NanoToRos,
       "declarative_bridge_zenoh_to_xrce"),

    // ── Imperative (issue #53) zenoh→cyclone bridge — the G4 blind spot the
    //    binding closes (phase-329 W3). Same coordinate as the declarative
    //    sibling, distinct test. ──────────────────────────────────────────
    ic("bridge-zenoh-to-cyclone-imperative",
       c(Linux, Rust, Zenoh, Pubsub, Bridge, Runtime),
       NativeFixtures, NanoBridge { ingress: Zenoh, egress: Cyclonedds }, NanoToRos,
       "bridge_zenoh_to_cyclonedds"),

    // ── Native nano ↔ stock ROS 2, the previously-unbound live-peer lanes
    //    (phase-329 W3). Each file mixes host-only cases with a ROS-2-facing
    //    lane; the coordinate below is the interop lane's. ─────────────────
    // tests/qos_override_e2e.rs — a plan QoS override reaches the ADVERTISED
    // profile a stock rmw_zenoh_cpp peer reads (issue #52/0303/0306).
    ic("native-qos-override-rust-zenoh",
       c(Linux, Rust, Zenoh, Qos, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), NanoToRos, "qos_override_e2e"),
    // tests/params.rs — `ros2 param list/get/set` against a nano params entry.
    ic("native-params-rust-zenoh",
       c(Linux, Rust, Zenoh, Params, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), BiDir, "params"),
    // tests/params_per_node_interop.rs — phase-426 W6, the cell that would have
    // caught three parameter stores coexisting. Same coordinate as its sibling
    // above and a DIFFERENT test, the shape `bridge-zenoh-to-cyclone{,-imperative}`
    // already has: the coordinate says what is built, not what is asked of it.
    // Its sibling drives one node, and one node is precisely what the phase-426
    // defect is invisible through — the six services were published under the
    // EXECUTOR's identity, so a single-node image reads the same before and
    // after. This one runs `ros2 param list/get/set` against a TWO-node image
    // and asserts each node's own FQN answers with its own value.
    ic("native-params-per-node-rust-zenoh",
       c(Linux, Rust, Zenoh, Params, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), BiDir, "params_per_node_interop"),
    // tests/rust_multi_node_per_node_graph.rs — a multi-node Rust entry shows
    // one graph node per launch component in `ros2 node list` (#104/phase-268).
    ic("native-multinode-rust-zenoh",
       c(Linux, Rust, Zenoh, EntryPubsub, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), NanoToRos, "rust_multi_node_per_node_graph"),
    // Issue 1269 — the SAME image on Cyclone (`[image.native_cyclonedds]`,
    // row `workspace-rust-native-cyclonedds`), asserting the SAME node set.
    // Cyclone announces nodes through `ros_discovery_info`, not liveliness
    // tokens, and published one session-named node for the whole image until
    // 1269 — so a green zenoh case says nothing about it.
    ic("native-multinode-rust-cyclone",
       c(Linux, Rust, Cyclonedds, EntryPubsub, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), NanoToRos, "rust_multi_node_per_node_graph"),
    // And XRCE's third, carved: the backend publishes no `ros_discovery_info`
    // at all, so a stock graph cache learns none of its nodes. The fixture
    // exists (`workspace-rust-native-xrce`); the missing piece is the backend.
    ic("native-multinode-rust-xrce-CARVED",
       c(Linux, Rust, Xrce, EntryPubsub, Interop,
         CarveOut("nros-rmw-xrce writes no `ros_discovery_info` and leaves \
                   `create_node` NULL, so `ros2 node list` has no source for \
                   an XRCE image's nodes — reasoned from session.c, not yet \
                   measured. A live Agent + peer would only confirm the gap. \
                   Issue 1292.")),
       NativeFixtures, XrceAgent, NanoToRos, NO_TEST),
    // tests/cpp_multi_node_entry.rs — the C++ typed multi-node entry's pubsub +
    // per-node graph visibility against a stock ROS 2 peer (phase-257/268).
    ic("native-multinode-cpp-zenoh",
       c(Linux, Cpp, Zenoh, EntryPubsub, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), NanoToRos, "cpp_multi_node_entry"),

    // ── phase-433 W6 (jobs 3–5) — what we ADVERTISE about ourselves ──────
    // tests/advertised_state_interop.rs. Matched counts, the publisher GID,
    // the actual-QoS read-back (issue 0823) and `get_serialization_format`:
    // six slots that are `produced` and had never met a peer. The same trap
    // phase-381 walked into — produced, mutation-tested, parity-clean, and the
    // feature did not work (issue 0903).
    //
    // Cyclonedds is not a choice. It is the only backend that fills any of
    // them: zenoh reaches the vtable through `RustBackendAdapter::VTABLE`,
    // which ends `..EMPTY_VTABLE`, and the XRCE / uORB initialisers stop
    // before them. There is no zenoh sibling to carve, because there is no
    // zenoh implementation to run.
    //
    // BiDir: the publisher half needs a peer that SUBSCRIBES (to move
    // `publisher_count_matched_subscriptions`) and the subscription half needs
    // one that PUBLISHES, on separate topics — a writer and a reader on one
    // topic in one participant match EACH OTHER, which would make the rise
    // from zero unobservable.
    ic("native-advertised-state-rust-cyclone-bidir",
       c(Linux, Rust, Cyclonedds, AdvertisedState, Interop, Runtime),
       NativeFixtures, RosEdition(Cyclonedds), BiDir, "advertised_state_interop"),

    // ── phase-433 W6 — a QoS STATUS EVENT fires against a live peer ──────
    // tests/qos_event_interop.rs. The four event slots (`publisher_event_init`,
    // `publisher_take_event`, `subscription_event_init`,
    // `subscription_take_event`) are `produced` and had never met a peer.
    //
    // They are the family least able to be tested in isolation: a QoS event's
    // input is a REMOTE entity's state. Four of the five kinds our ABI defines
    // are, in the zenoh shim, comparisons of our own clock against our own
    // timestamps — `LivelinessLost` and `OfferedDeadlineMissed` on the publisher
    // are literally self-observations. `LivelinessChanged` is the one whose
    // trigger is another process: the set of publishers holding an `@ros2_lv`
    // token matching the topic. That makes it the only INTEROP claim in the
    // family, because the token is written by `rmw_zenoh_cpp` and matched by our
    // wildcard, and the tree's only existing check of that wildcard matches it
    // against a keyexpr OUR OWN builder produced.
    //
    // Note what our ABI does NOT have: an incompatible-QoS kind. Upstream's
    // `RMW_EVENT_OFFERED_QOS_INCOMPATIBLE` / `REQUESTED_QOS_INCOMPATIBLE` have no
    // counterpart in `rmw_event_type_t`, so the cheapest event to provoke from a
    // stock peer is not one we can represent.
    ic("native-qos-event-rust-zenoh-r2n",
       c(Linux, Rust, Zenoh, QosEvents, Interop, Runtime),
       NativeFixtures, RosEdition(Zenoh), RosToNano, "qos_event_interop"),
    // Cyclone's half, carved rather than run — and the carve-out IS the W6
    // finding for that backend, measured from `vtable.cpp` rather than assumed.
    //
    // `subscription_event_init` and `publisher_event_init` are NULL there
    // (`kRegisterSubscriptionEvent` / `kRegisterPublisherEvent`, "deferred"
    // since phase 108). The other two slots, `subscription_take_event` and
    // `publisher_take_event`, ARE implemented and read real
    // `dds_get_*_status` counters — but nothing calls them: the Rust adapter
    // sets both to `None`, `nros-node` exposes no poll API, and a repo-wide
    // grep for `take_event` finds no consumer outside cyclonedds' own
    // `tests/status_events.cpp`. So a Cyclone application cannot observe a
    // status event by either half of the surface, and a live peer cannot change
    // that; the missing piece is a runtime poll path, not a lane.
    ic("native-qos-event-rust-cyclone-r2n-CARVED",
       c(Linux, Rust, Cyclonedds, QosEvents, Interop,
         CarveOut("cyclonedds cannot deliver a QoS status event to an application \
                   at all: both `*_event_init` slots are NULL, and the \
                   `*_take_event` pair it does implement has no caller in the \
                   tree outside its own C++ unit test. A live peer would change \
                   nothing — the gap is a runtime poll path. Issue 1164.")),
       NativeFixtures, RosEdition(Cyclonedds), RosToNano, NO_TEST),
];

/// `cell` sentinel for a case that is evidence for NO interop cell.
pub const NO_CELL: &str = "(no cell — evidence for none)";

/// One CASE of a shared interop binary and the cell it is evidence FOR —
/// issue 1191.
///
/// Eleven binaries host the nineteen Runtime cells, and four of them host more
/// than one: `interop_e2e` (5), `graph_interop` (2), `ros2_action_e2e` (2),
/// `xrce_ros2_interop` (2). Until this table existed, the verdict ledger
/// (`scripts/check-interop-verdicts.py`) attributed a junit case to the BINARY,
/// so every case of `interop_e2e` was evidence — and counter-evidence — for all
/// five of its cells at once: one failing case (issue 1190's
/// `case_2_zenoh_pubsub_ros2_to_nano`) refused a recording for the four cells
/// whose own cases had all passed.
///
/// A coordinate cannot do this job. `native-action-rust-cyclone-{r2n,n2r}` are
/// ONE `(Linux, Rust, Cyclonedds, Action)` coordinate in ONE binary and differ
/// only by direction, which [`coords_for`] collapses on purpose. Nor may a case
/// be matched to a cell by NAME: a cell id and a case name are different
/// vocabularies, and a substring rule reads as working until a rename.
///
/// So the assignment is WRITTEN DOWN here, and gated from both ends:
///
/// * Rust (below): every `cell` is a real `Tier::Runtime` cell of that same
///   binary, no `(test, case)` pair is claimed twice, and every cell of a
///   shared binary owns at least one case — a cell owning none could never be
///   recorded again.
/// * Python (`check-interop-verdicts.py`): the case names here are exactly the
///   cases the binary's source actually runs — derived from `#[test]` /
///   `#[rstest]` and rstest's own `case_<n>_<description>` naming — so a
///   renamed, added or deleted case turns the gate RED instead of silently
///   dropping or inventing evidence. A junit that carries a case this table
///   does not name is REFUSED at `--record` time for the same reason.
///
/// `case` is spelled AS THE JUNIT SPELLS IT: `<fn>` for a plain `#[test]`, and
/// `<fn>::case_<n>_<description>` for an rstest `#[case::<description>(…)]`
/// (`n` left-zero-padded to the width of the case count, which is rstest's
/// rule, not ours).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CaseOwner {
    /// The test binary — an `ic(...)` row's `test`.
    pub test: &'static str,
    /// The case, as the junit spells it.
    pub case: &'static str,
    /// The [`CELLS`] id this case is evidence for, or [`NO_CELL`].
    pub cell: &'static str,
    /// Why no cell owns it. Non-empty exactly when `cell` is [`NO_CELL`] — an
    /// unowned case is a DECISION (a lane that does not exist), not an
    /// omission, and it stops being evidence for anybody.
    pub reason: &'static str,
}

const fn co(test: &'static str, case: &'static str, cell: &'static str) -> CaseOwner {
    CaseOwner {
        test,
        case,
        cell,
        reason: "",
    }
}

const fn unowned(test: &'static str, case: &'static str, reason: &'static str) -> CaseOwner {
    CaseOwner {
        test,
        case,
        cell: NO_CELL,
        reason,
    }
}

/// The case → cell map for every binary that hosts more than one Runtime cell.
///
/// A binary listed here is mapped EXHAUSTIVELY: the Python gate requires its
/// rows to cover exactly the cases the source runs (binding tests excluded —
/// they are evidence of nothing). A binary with a single Runtime cell needs no
/// rows: its every case is that cell's, which is what the ledger already
/// assumed. Such a binary gaining a second cell turns the gate red, which is
/// the moment the map becomes necessary.
#[rustfmt::skip]
pub const CASE_CELLS: &[CaseOwner] = &[
    // ── interop_e2e — five cells, nine cases ────────────────────────────
    // Each row is `scenario_coord()`'s answer for that case's `Scenario`
    // (tests/interop_e2e.rs), and those five coordinates are distinct, so the
    // assignment is the one the per-case tripwire already asserts — written
    // where a tool without a compiler can read it. Direction does NOT split a
    // cell: `case_2`/`case_3` run ROS-2-to-nano against the cell whose id ends
    // `-n2r`, exactly as `coords_for` collapses the two.
    co("interop_e2e", "interop::case_1_zenoh_pubsub_nano_to_ros2",      "native-pubsub-rust-zenoh-n2r"),
    co("interop_e2e", "interop::case_2_zenoh_pubsub_ros2_to_nano",      "native-pubsub-rust-zenoh-n2r"),
    co("interop_e2e", "interop::case_3_zenoh_pubsub_stock_demo_nodes_cpp", "native-pubsub-rust-zenoh-n2r"),
    co("interop_e2e", "interop::case_4_zenoh_service_nano_server",      "native-service-rust-zenoh-r2n"),
    co("interop_e2e", "interop::case_5_zenoh_service_ros2_server",      "native-service-rust-zenoh-r2n"),
    co("interop_e2e", "interop::case_6_cyclone_pubsub_nano_to_ros2",    "native-pubsub-c-cyclone-n2r"),
    co("interop_e2e", "interop::case_7_cyclone_pubsub_ros2_to_nano",    "native-pubsub-c-cyclone-n2r"),
    co("interop_e2e", "interop::case_8_cyclone_service_nano_server",    "native-service-c-cyclone-r2n"),
    co("interop_e2e", "interop::case_9_zenoh_lifecycle_full_cycle",     "native-lifecycle-rust-zenoh"),

    // ── graph_interop — one case per RMW, and the RMW is in the body ────
    // (`require_ros2()` vs `require_ros2_cyclonedds()`), not only in the name.
    co("graph_interop", "nano_ros_enumerates_a_stock_ros2_node", "native-graph-rust-zenoh-r2n"),
    co("graph_interop", "cyclone_enumerates_a_stock_ros2_node",  "native-graph-rust-cyclone-r2n"),

    // ── rust_multi_node_per_node_graph — one image, one case per RMW ────
    // Issue 1269. Each case names its backend in the body (the fixture it
    // resolves and the `ros2` env it lists through), not only in the name.
    co("rust_multi_node_per_node_graph", "rust_multi_node_entry_per_node_graph_nodes",
       "native-multinode-rust-zenoh"),
    co("rust_multi_node_per_node_graph", "rust_multi_node_entry_per_node_graph_nodes_cyclonedds",
       "native-multinode-rust-cyclone"),

    // ── ros2_action_e2e — ONE coordinate, two directions ────────────────
    // The pair no coordinate can separate: which side drives is the whole
    // difference between the two cells, and it is what each case does.
    co("ros2_action_e2e", "a_stock_ros2_client_drives_the_nano_ros_action_server",
       "native-action-rust-cyclone-r2n"),
    co("ros2_action_e2e", "the_nano_ros_action_client_drives_a_stock_ros2_server",
       "native-action-rust-cyclone-n2r"),

    // ── xrce_ros2_interop — pubsub, service, and three cases with no cell ─
    co("xrce_ros2_interop", "test_xrce_to_ros2_pubsub",      "native-pubsub-rust-xrce-n2r"),
    co("xrce_ros2_interop", "test_ros2_to_xrce_pubsub",      "native-pubsub-rust-xrce-n2r"),
    co("xrce_ros2_interop", "test_xrce_service_ros2_client", "native-service-rust-xrce-r2n"),
    co("xrce_ros2_interop", "test_ros2_service_xrce_client", "native-service-rust-xrce-r2n"),
    // The file runs three ACTION cases and `interop::CELLS` declares no
    // Xrce/Action cell — `cases_bound_to_interop_cells` names Pubsub and
    // Service only, and a coordinate set cannot show what has no row. They ran
    // as the pubsub and service cells' evidence until this table said
    // otherwise. Recorded rather than assigned: the gap is a missing CELL, not
    // a missing case.
    unowned("xrce_ros2_interop", "test_xrce_action_ros2_client",
            "no Xrce/Action interop cell exists; the case runs, and nothing in \
             interop::CELLS claims its coordinate"),
    unowned("xrce_ros2_interop", "test_xrce_action_ros2_concurrent",
            "no Xrce/Action interop cell exists (the concurrent-goal variant of \
             the case above)"),
    unowned("xrce_ros2_interop", "test_ros2_action_xrce_client",
            "no Xrce/Action interop cell exists (the ROS-2-drives-nano direction \
             of the two above)"),
];

/// Which cell, if any, a case of `test` is evidence for. `None` when the binary
/// carries no map (it has one cell, so every case is that cell's) — callers
/// that need the difference ask [`is_mapped`] first.
pub fn owner_of_case(test: &str, case: &str) -> Option<&'static CaseOwner> {
    CASE_CELLS.iter().find(|o| o.test == test && o.case == case)
}

/// Does this binary carry a case → cell map?
pub fn is_mapped(test: &str) -> bool {
    CASE_CELLS.iter().any(|o| o.test == test)
}

/// The cases declared as evidence for one cell id.
pub fn cases_of(cell_id: &str) -> impl Iterator<Item = &'static str> {
    CASE_CELLS
        .iter()
        .filter(move |o| o.cell == cell_id)
        .map(|o| o.case)
}

/// Runtime interop/bridge cells only.
pub fn runtime_cells() -> impl Iterator<Item = &'static InteropCell> {
    CELLS
        .iter()
        .filter(|ic| matches!(ic.cell.tier, Tier::Runtime))
}

/// A nano coordinate `(platform, lang, rmw, workload)` — the granularity a test
/// binds to. Directions collapse (a cell may be exercised by an N2R and an R2N
/// case), so the binding is at coordinate level, not per-case.
pub type Coord = (u16, u16, u16, u16);

/// The distinct Runtime coordinates the given test's cells cover, per
/// `interop::CELLS`.
pub fn coords_for(test: &str) -> std::collections::BTreeSet<Coord> {
    CELLS
        .iter()
        .filter(|ic| ic.test == test && matches!(ic.cell.tier, Tier::Runtime))
        .map(|ic| {
            (
                ic.cell.platform.index(),
                ic.cell.lang.port_index(),
                ic.cell.rmw.index(),
                ic.cell.workload.port_offset(),
            )
        })
        .collect()
}

/// The interop cell with this id, if any.
pub fn by_id(id: &str) -> Option<&'static InteropCell> {
    CELLS.iter().find(|ic| ic.id == id)
}

/// Does `test` declare a Runtime cell at coordinate `(p, l, r, w)`? The runtime
/// per-case binding (issue 0352 / phase-324 W4.d): a test asserts, for each case
/// it actually runs, that the coordinate that case exercises is declared for it
/// in `interop::CELLS`. A case running a coordinate the SSoT does not list — the
/// 0341 defect-2 drift, seen from the test side — fails the running case.
pub fn test_covers(
    test: &str,
    p: PlatformId,
    l: crate::matrix::Lang,
    r: Rmw,
    w: crate::matrix::Workload,
) -> bool {
    coords_for(test).contains(&(p.index(), l.port_index(), r.index(), w.port_offset()))
}

/// Bind an interop test to `interop::CELLS`: assert the coordinates its `#[case]`s
/// exercise (`covered`, kept adjacent to the cases) are exactly those the list
/// declares for `test` (issue 0352 / phase-324 W4). Adding/retiring/mutating an
/// interop cell without tracking the test — or a test drifting from its cell's
/// declared coordinate (issue 0341 defect 2) — turns this RED.
///
/// Call it from a `#[test]` in the test binary; it needs no fixtures, so it runs
/// in tier 1 regardless of whether the runtime lane's ROS 2 / docker / QEMU
/// dependencies are present.
pub fn assert_test_bound(
    test: &str,
    covered: &[(
        PlatformId,
        crate::matrix::Lang,
        Rmw,
        crate::matrix::Workload,
    )],
) {
    let declared = coords_for(test);
    let actual: std::collections::BTreeSet<Coord> = covered
        .iter()
        .map(|(p, l, r, w)| (p.index(), l.port_index(), r.index(), w.port_offset()))
        .collect();
    assert_eq!(
        actual, declared,
        "interop test `{test}`: its #[case]s cover coordinates {actual:?}, but \
         interop::CELLS declares {declared:?} for this test — keep the cases and \
         interop::CELLS in sync (add/retire the row, or fix the drifted coordinate)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cell here is Interop or Bridge — the whole point of the split.
    #[test]
    fn only_interop_or_bridge_kinds() {
        for c in CELLS {
            assert!(
                matches!(c.cell.kind, Kind::Interop | Kind::Bridge),
                "non-interop/bridge cell in interop::CELLS: {c:?}"
            );
        }
    }

    /// Stable ids are unique — they are the `Binding` key.
    #[test]
    fn ids_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in CELLS {
            assert!(seen.insert(c.id), "duplicate interop cell id: {}", c.id);
        }
    }

    /// Issue 1191 — every mapped case names a real Runtime cell OF THAT
    /// BINARY. A row pointing at another binary's cell, or at a cell that no
    /// longer exists, would attribute a live result to the wrong place, which
    /// is worse than the undercount the map replaces.
    #[test]
    fn case_owners_name_a_runtime_cell_of_their_own_binary() {
        for o in CASE_CELLS {
            if o.cell == NO_CELL {
                assert!(
                    !o.reason.is_empty(),
                    "`{}` of `{}` owns no cell and says no why — an unowned case \
                     is a decision, not an omission",
                    o.case,
                    o.test
                );
                continue;
            }
            assert!(
                o.reason.is_empty(),
                "`{}` of `{}` names cell `{}` AND a reason; the reason field is \
                 for NO_CELL rows only",
                o.case,
                o.test,
                o.cell
            );
            let cell = by_id(o.cell)
                .unwrap_or_else(|| panic!("no interop cell `{}` (case `{}`)", o.cell, o.case));
            assert!(
                matches!(cell.cell.tier, Tier::Runtime),
                "case `{}` names `{}`, which is not Tier::Runtime",
                o.case,
                o.cell
            );
            assert_eq!(
                cell.test, o.test,
                "case `{}` of `{}` names cell `{}`, whose test binary is `{}`",
                o.case, o.test, o.cell, cell.test
            );
        }
    }

    /// One case, one owner. Two rows for the same case would make a result
    /// count twice — and, with different cells, count for a cell that did not
    /// produce it.
    #[test]
    fn case_owners_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for o in CASE_CELLS {
            assert!(
                seen.insert((o.test, o.case)),
                "case `{}` of `{}` is claimed twice",
                o.case,
                o.test
            );
        }
    }

    /// Every binary hosting more than one Runtime cell is mapped, and every one
    /// of its cells owns at least one case. A cell owning none could never be
    /// recorded from a run again — the undercount of issue 1191, made
    /// permanent.
    #[test]
    fn every_shared_binary_maps_all_of_its_cells() {
        let mut per_test: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for c in runtime_cells() {
            per_test.entry(c.test).or_default().push(c.id);
        }
        for (test, ids) in per_test {
            if ids.len() < 2 {
                continue;
            }
            assert!(
                is_mapped(test),
                "`{test}` hosts {} Runtime cells ({ids:?}) and no CASE_CELLS row \
                 says which case is whose — every case would be evidence for \
                 every one of them (issue 1191)",
                ids.len()
            );
            for id in ids {
                assert!(
                    cases_of(id).next().is_some(),
                    "cell `{id}` shares binary `{test}` with others and owns no \
                     case — nothing could ever record it"
                );
            }
        }
    }

    /// Every carve-out reason is non-empty (audit E5, mirrored from matrix.rs).
    #[test]
    fn gap_tiers_carry_reasons() {
        for c in CELLS {
            if let Tier::CarveOut(r) | Tier::BuildOnly(r) = c.cell.tier {
                assert!(!r.is_empty(), "empty reason: {c:?}");
            }
        }
    }
}

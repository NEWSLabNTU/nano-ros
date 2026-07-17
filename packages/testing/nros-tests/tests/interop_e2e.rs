//! phase-295 W6.c — THE ROS 2 interop matrix consumer (RFC-0051 §2 + §W6.c).
//!
//! Consolidates the ROS 2 interop family — `rmw_interop.rs` (zenoh),
//! `cyclonedds_ros2_interop.rs` (cyclone), `demo_nodes_cpp_interop.rs`
//! (cross-vendor stock rclcpp talker), and `ros2_lifecycle_interop.rs`
//! (lifecycle) — into one parametrized test over the `Kind::Interop` cells
//! of the test matrix (`nros_tests::matrix`): a nano-ros node exchanges data
//! with a REAL ROS 2 peer, over the reduced workload set (pubsub + service,
//! plus lifecycle) × direction (nano-pub/ros-sub, ros-pub/nano-sub) × RMW
//! (rmw_zenoh_cpp, rmw_cyclonedds_cpp).
//!
//! **Behavioral interchangeability (RFC-0051 §2).** The shared
//! [`nros_tests::checker::assert_delivery`] reads process output and is
//! transport/peer-agnostic, so every nano-ros endpoint (and the stock
//! `demo_nodes_cpp` peer) is asserted through it — the same contract the
//! nano↔nano example cells pin. The raw `ros2 topic echo` / `ros2 service
//! call` sinks are NOT demo nodes; they dump the DDS/CLI wire fields
//! (`data:`, `sum`), so those directions count wire samples with the
//! DDS/CLI markers (which are NOT nano demo markers — the output-marker gate
//! only guards the nano demo wording in `output.rs`).
//!
//! Skip semantics are preserved EXACTLY from the per-cell files: the zenoh +
//! lifecycle cells gate on `require_ros2` (ROS 2 CLI + `rmw_zenoh_cpp`) and a
//! startable `zenohd`; the cyclone cells gate on `require_ros2_cyclonedds`
//! (ROS 2 + `rmw_cyclonedds_cpp`) + the native Cyclone fixtures. A missing
//! ROS 2 / RMW / fixture / peer-launch is a clean `skip!`, never a failure —
//! after the gate passes, ZERO delivery is a real failure (#133 fail-loud).
//!
//! Isolation: zenoh cells take an EPHEMERAL router
//! (`ZenohRouter::start_unique`, `NROS_LOCATOR`); cyclone cells take a
//! PID-seeded `unique_ros_domain_id()` `ROS_DOMAIN_ID` so no two concurrent
//! interop tests share a domain (across RMWs too). The whole binary runs in
//! the `ros2-interop` nextest group (singleton ros2 daemon = the exclusive
//! resource; max-threads = 1); the cyclone cases route to
//! `host-dds-ros2-interop` (shared 232-slot DDS domain space) via a
//! `test(cyclone)` filter.
//!
//! Bespoke interop lanes kept OUT of this consumer (their own binaries):
//! `xrce_ros2_interop.rs` (XRCE Agent lifecycle specifics) and
//! `qos_zephyr_ros2_interop_e2e.rs` (the on-target zephyr-image QoS interop,
//! `zephyr-qos-port` nextest group). See the phase-295 W6.c doc for the
//! introspection/benchmark lanes retired in the reduction.
//!
//! Run with: `cargo nextest run -p nros-tests --test interop_e2e`
//! (one RMW: `-E 'binary(interop_e2e) and test(cyclone)'`).

use nros_tests::{
    checker::assert_delivery,
    count_pattern,
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, Rmw as FixtureRmw, Ros2DdsProcess, Ros2Process,
        ZenohRouter, build_native_c_example_rmw, build_ros2_string_interop, lifecycle_node_binary,
        listener_binary, require_ros2, require_ros2_cyclonedds, ros2_env_setup_with_locator,
        service_client_binary, service_server_binary, talker_binary,
    },
    matrix::Workload,
    output, skip,
};
use rstest::rstest;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const TOPIC: &str = "/chatter";
const STRING_MSG: &str = "std_msgs/msg/String";
const SRV: &str = "/add_two_ints";
const SRV_TYPE: &str = "example_interfaces/srv/AddTwoInts";

// =============================================================================
// Cell table
// =============================================================================

/// The exact interop scenario the cell drives (RMW × workload × direction).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Scenario {
    /// zenoh: nano talker → `ros2 topic echo` (rmw_zenoh_cpp).
    ZenohPubsubNanoToRos2,
    /// zenoh: `ros2 topic pub` (rmw_zenoh_cpp) → nano listener.
    ZenohPubsubRos2ToNano,
    /// zenoh cross-vendor: a STOCK unmodified `demo_nodes_cpp talker`
    /// (rclcpp) → nano listener — the phase-211 "behaves like real ROS"
    /// proof (`demo_nodes_cpp_interop.rs`).
    ZenohPubsubStockDemoToNano,
    /// zenoh: nano service server ↔ `ros2 service call` (rmw_zenoh_cpp).
    ZenohServiceNanoServer,
    /// zenoh: `ros2` AddTwoInts server ↔ nano service client.
    ZenohServiceRos2Server,
    /// cyclone: nano talker → `ros2 topic echo` (rmw_cyclonedds_cpp).
    CyclonePubsubNanoToRos2,
    /// cyclone: `ros2 topic pub` (rmw_cyclonedds_cpp) → nano listener.
    CyclonePubsubRos2ToNano,
    /// cyclone: nano service server ↔ `ros2 service call` (rmw_cyclonedds_cpp).
    CycloneServiceNanoServer,
    /// zenoh: drive an nros lifecycle node through the REP-2002 service
    /// surface via `ros2 lifecycle …` (`ros2_lifecycle_interop.rs`).
    ZenohLifecycle,
}

impl Scenario {
    /// `true` for the cyclone cells (`test(cyclone)` routes them to the
    /// `host-dds-ros2-interop` nextest group).
    fn is_cyclone(self) -> bool {
        matches!(
            self,
            Scenario::CyclonePubsubNanoToRos2
                | Scenario::CyclonePubsubRos2ToNano
                | Scenario::CycloneServiceNanoServer
        )
    }
}

/// One interop matrix cell.
struct Cell {
    scenario: Scenario,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Skip-precondition gate: cyclone cells need ROS 2 + `rmw_cyclonedds_cpp`;
/// zenoh + lifecycle cells need ROS 2 + `rmw_zenoh_cpp`. Identical semantics
/// to the pre-consolidation files.
fn require_cell_env(scenario: Scenario) {
    if scenario.is_cyclone() {
        if !require_ros2_cyclonedds() {
            skip!("ROS 2 + rmw_cyclonedds_cpp not available");
        }
    } else if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
}

/// Start an ephemeral zenohd for the zenoh cells; a missing/unstartable
/// zenohd is a clean skip (the SUT is the interop, not the router).
fn start_zenoh_router() -> ZenohRouter {
    ZenohRouter::start_unique().unwrap_or_else(|e| skip!("zenohd failed to start: {e}"))
}

/// Spawn a native nano-ros zenoh binary dialing `locator`.
fn spawn_nano_zenoh(bin: &Path, name: &str, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info").env("NROS_LOCATOR", locator);
    ManagedProcess::spawn_command(cmd, name).unwrap_or_else(|e| panic!("spawn {name}: {e}"))
}

/// Resolve (building if needed) a native Cyclone C example binary, or skip
/// when the fixtures aren't set up (`just cyclonedds setup`).
fn nano_cyclone_c_binary(case: &str, binary: &str) -> std::path::PathBuf {
    build_native_c_example_rmw(case, binary, FixtureRmw::Cyclonedds).unwrap_or_else(|e| {
        skip!("native/c/{case} cyclonedds fixture not built (run `just cyclonedds setup`): {e:?}")
    })
}

/// Spawn a nano-ros Cyclone binary on `domain_id`, wiring `LD_LIBRARY_PATH`
/// to the in-tree `libddsc` (mirrors `native_api.rs::spawn_cyclone_binary`).
fn spawn_nano_cyclone(binary: &Path, name: &str, domain_id: u8) -> ManagedProcess {
    let mut cmd = Command::new(binary);
    cmd.env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("RUST_LOG", "info");
    let cyclone_lib = nros_tests::project_root().join("build/install/lib");
    let ld = match std::env::var_os("LD_LIBRARY_PATH") {
        Some(existing) if !existing.is_empty() => {
            let mut paths = vec![cyclone_lib];
            paths.extend(std::env::split_paths(&existing));
            std::env::join_paths(paths).expect("valid LD_LIBRARY_PATH")
        }
        _ => cyclone_lib.into_os_string(),
    };
    cmd.env("LD_LIBRARY_PATH", ld);
    ManagedProcess::spawn_command(cmd, name).unwrap_or_else(|_| panic!("Failed to start {name}"))
}

/// Run `ros2 <subcommand>` against `locator` (rmw_zenoh overlay); return
/// combined stdout+stderr. Used by the lifecycle cell — `--no-daemon` is
/// passed by the caller so the CLI uses this process' zenoh session.
fn run_ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("failed to spawn bash for ros2 invocation");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output contains `marker`
/// (case-insensitive) or timeout.
fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let marker = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < deadline {
        last = run_ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    last
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// One interop cell: exchange data between a nano-ros node and a real ROS 2
/// peer, asserting delivery per the scenario. Case names carry the
/// `<rmw>_<workload>_<direction>` shape so nextest `test(...)` filters can
/// slice by RMW (`test(cyclone)` routes to the host-DDS group).
#[rstest]
// ── zenoh (rmw_zenoh_cpp) — matrix (Native, Rust, Zenoh, {Pubsub,Service}, Interop)
#[case::zenoh_pubsub_nano_to_ros2(Cell {
    scenario: Scenario::ZenohPubsubNanoToRos2,
    note: "#133 fail-loud: after require_ros2, the ros2 subscriber receiving 0 \
           `data:` samples is a real rmw_zenoh delivery failure, not timing",
})]
#[case::zenoh_pubsub_ros2_to_nano(Cell {
    scenario: Scenario::ZenohPubsubRos2ToNano,
    note: "#146: rmw_zenoh pub → zenoh-pico sub discovery is ~10 s (25 s window); \
           data integrity checked once delivery held",
})]
#[case::zenoh_pubsub_stock_demo_nodes_cpp(Cell {
    scenario: Scenario::ZenohPubsubStockDemoToNano,
    note: "phase-211: an UNMODIFIED stock demo_nodes_cpp talker (rclcpp) reaches a \
           nano-ros subscriber cross-vendor over a shared zenohd",
})]
#[case::zenoh_service_nano_server(Cell {
    scenario: Scenario::ZenohServiceNanoServer,
    note: "#133 fail-loud: the ros2 client must receive a `sum` reply (5 + 3 = 8)",
})]
#[case::zenoh_service_ros2_server(Cell {
    scenario: Scenario::ZenohServiceRos2Server,
    note: "#133 fail-loud: the nano client must receive the ros2 AddTwoInts reply",
})]
// ── cyclone (rmw_cyclonedds_cpp) — matrix (Native, {C}, Cyclonedds, {Pubsub,Service}, Interop)
#[case::cyclone_pubsub_nano_to_ros2(Cell {
    scenario: Scenario::CyclonePubsubNanoToRos2,
    note: "phase-117/183.5: nano-ros Cyclone talker is wire-compatible with stock \
           rmw_cyclonedds_cpp over RTPS/SPDP",
})]
#[case::cyclone_pubsub_ros2_to_nano(Cell {
    scenario: Scenario::CyclonePubsubRos2ToNano,
    note: "phase-183.5: setvbuf(_IOLBF) in examples/native/c/listener made the \
           `I heard:` lines reach the harness (block-buffering, not a wire gap)",
})]
#[case::cyclone_service_nano_server(Cell {
    scenario: Scenario::CycloneServiceNanoServer,
    note: "phase-117.12.B.1: write the reply once the reply reader is DISCOVERED \
           (total_count > 0), not only on current_count > 0 (src/service.cpp)",
})]
// ── zenoh lifecycle — matrix (Native, Rust, Zenoh, Lifecycle, Interop)
#[case::zenoh_lifecycle_full_cycle(Cell {
    scenario: Scenario::ZenohLifecycle,
    note: "the REP-2002 service surface driven end-to-end via `ros2 lifecycle …` \
           (nodes/get/set configure/list) against an nros lifecycle node",
})]
fn interop(#[case] cell: Cell) {
    require_cell_env(cell.scenario);

    match cell.scenario {
        // ── zenoh pubsub: nano talker → ros2 topic echo ──────────────────
        Scenario::ZenohPubsubNanoToRos2 => {
            let router = start_zenoh_router();
            let locator = router.locator();
            let mut ros2 =
                match Ros2Process::topic_echo(TOPIC, STRING_MSG, &locator, DEFAULT_ROS_DISTRO) {
                    Ok(p) => p,
                    Err(e) => skip!("ROS 2 topic echo could not start: {e}"),
                };
            let mut talker = spawn_nano_zenoh(&talker_binary(), "native-rs-talker", &locator);
            let out = ros2
                .wait_for_output(Duration::from_secs(8))
                .unwrap_or_default();
            talker.kill();

            let n = count_pattern(&out, "data:");
            assert!(
                n > 0,
                "nros → ROS 2 delivered nothing: the ros2 subscriber received 0 `data:` \
                 samples from the nano talker over rmw_zenoh ({}).\nROS 2 output:\n{out}",
                cell.note
            );
        }

        // ── zenoh pubsub: ros2 topic pub → nano listener ─────────────────
        Scenario::ZenohPubsubRos2ToNano => {
            let router = start_zenoh_router();
            let locator = router.locator();
            let mut listener = spawn_nano_zenoh(&listener_binary(), "native-rs-listener", &locator);
            listener
                .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
                .expect("nros listener did not become ready");

            let mut ros2 = match Ros2Process::topic_pub(
                TOPIC,
                STRING_MSG,
                "{data: 'Hello World: 42'}",
                1,
                &locator,
                DEFAULT_ROS_DISTRO,
            ) {
                Ok(p) => p,
                Err(e) => {
                    listener.kill();
                    skip!("ROS 2 publisher could not start: {e}");
                }
            };

            // #146 — rmw_zenoh pub → zenoh-pico sub discovery is ~10 s.
            let out = listener
                .wait_for_output_count(output::LISTENER_LOG_PREFIX, 1, Duration::from_secs(25))
                .unwrap_or_default();
            ros2.kill();
            listener.kill();

            assert_delivery(Workload::Pubsub, &out, 1);
            assert!(
                out.contains("Hello World: 42"),
                "ROS 2 → nros data integrity: payload 'Hello World: 42' missing ({}).\n\
                 nros output:\n{out}",
                cell.note
            );
        }

        // ── zenoh pubsub cross-vendor: stock demo_nodes_cpp talker → nano ─
        Scenario::ZenohPubsubStockDemoToNano => {
            let router = start_zenoh_router();
            let locator = router.locator();
            let sub_bin = build_ros2_string_interop()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|e| skip!("ros2-string-interop fixture not built: {e}"));

            // nano subscriber first, so its /chatter subscription is declared
            // before the stock talker publishes.
            let mut sub_cmd = Command::new(&sub_bin);
            sub_cmd
                .env("RUST_LOG", "info")
                .env("NROS_LOCATOR", &locator)
                .env("NROS_SESSION_MODE", "client");
            let mut sub = ManagedProcess::spawn_command(sub_cmd, "nros-string-sub")
                .expect("spawn nano-ros sub");
            sub.wait_for_output_pattern("Waiting for", Duration::from_secs(8))
                .expect("nano-ros subscriber did not become ready");

            let mut talker = Ros2Process::demo_nodes_cpp_talker(&locator, DEFAULT_ROS_DISTRO)
                .expect("spawn demo_nodes_cpp talker");

            // This nano sub is the raw-Int32 `ros2-string-interop` bin
            // (prints `Received:`, not the demo `I heard:`), so it counts
            // the INT32 listener marker directly.
            let out = sub
                .wait_for_output_count(
                    output::INT32_LISTENER_LOG_PREFIX,
                    2,
                    Duration::from_secs(20),
                )
                .unwrap_or_default();
            sub.kill();
            talker.kill();

            let received = count_pattern(&out, output::INT32_LISTENER_LOG_PREFIX);
            assert!(
                received >= 2,
                "nano-ros must receive the stock demo_nodes_cpp talker cross-vendor \
                 (received = {received}) ({}).\n{out}",
                cell.note
            );
        }

        // ── zenoh service: nano server ↔ ros2 service call ───────────────
        Scenario::ZenohServiceNanoServer => {
            let router = start_zenoh_router();
            let locator = router.locator();
            let mut server = spawn_nano_zenoh(
                &service_server_binary(),
                "native-rs-service-server",
                &locator,
            );
            let _ = server.wait_for_output_pattern(
                output::SERVICE_SERVER_READY_MARKER,
                Duration::from_secs(5),
            );
            if !server.is_running() {
                panic!("native-rs-service-server (the nros SUT) exited before service-ready");
            }

            let mut client = match Ros2Process::service_call(
                SRV,
                SRV_TYPE,
                "{a: 5, b: 3}",
                &locator,
                DEFAULT_ROS_DISTRO,
            ) {
                Ok(p) => p,
                Err(e) => {
                    server.kill();
                    skip!("ROS 2 service call could not start: {e}");
                }
            };
            let out = client
                .wait_for_output(Duration::from_secs(10))
                .unwrap_or_default();
            server.kill();

            assert!(
                out.contains("sum"),
                "nros service server ↔ ROS 2 client got no `sum` response (5 + 3 = 8) \
                 ({}).\nROS 2 output:\n{out}",
                cell.note
            );
        }

        // ── zenoh service: ros2 server ↔ nano client ─────────────────────
        Scenario::ZenohServiceRos2Server => {
            let router = start_zenoh_router();
            let locator = router.locator();
            let mut ros2_server =
                match Ros2Process::add_two_ints_server(&locator, DEFAULT_ROS_DISTRO) {
                    Ok(p) => p,
                    Err(e) => skip!("ROS 2 service server could not start: {e}"),
                };
            let mut client = spawn_nano_zenoh(
                &service_client_binary(),
                "native-rs-service-client",
                &locator,
            );
            let out = client
                .wait_for_all_output(Duration::from_secs(15))
                .unwrap_or_default();
            ros2_server.kill();

            // The nano client prints the demo `Result of add_two_ints:` line.
            assert_delivery(Workload::Service, &out, 1);
        }

        // ── cyclone pubsub: nano talker → ros2 topic echo ────────────────
        Scenario::CyclonePubsubNanoToRos2 => {
            let domain = nros_tests::unique_ros_domain_id();
            let talker_bin = nano_cyclone_c_binary("talker", "c_talker");
            let mut ros2 = Ros2DdsProcess::topic_echo_cyclonedds_with_domain(
                TOPIC,
                STRING_MSG,
                DEFAULT_ROS_DISTRO,
                domain,
            )
            .expect("start ros2 cyclone echo");
            std::thread::sleep(Duration::from_secs(2));
            let mut talker = spawn_nano_cyclone(&talker_bin, "nano-cyclone-talker", domain);

            let out = ros2
                .wait_for_output(Duration::from_secs(10))
                .unwrap_or_default();
            talker.kill();

            let n = count_pattern(&out, "data:");
            assert!(
                n > 0,
                "ROS 2 cyclone subscriber received no `data:` samples from the nano \
                 talker ({}).\n{out}",
                cell.note
            );
        }

        // ── cyclone pubsub: ros2 topic pub → nano listener ───────────────
        Scenario::CyclonePubsubRos2ToNano => {
            let domain = nros_tests::unique_ros_domain_id();
            let listener_bin = nano_cyclone_c_binary("listener", "c_listener");
            let mut listener = spawn_nano_cyclone(&listener_bin, "nano-cyclone-listener", domain);
            std::thread::sleep(Duration::from_secs(3));
            let mut ros2 = Ros2DdsProcess::topic_pub_cyclonedds_with_domain(
                TOPIC,
                STRING_MSG,
                "{data: 'Hello World: 42'}",
                5,
                DEFAULT_ROS_DISTRO,
                domain,
            )
            .expect("start ros2 cyclone pub");

            let out = listener
                .wait_for_output_pattern(output::LISTENER_LOG_PREFIX, Duration::from_secs(10))
                .unwrap_or_default();
            ros2.kill();
            listener.kill();

            assert_delivery(Workload::Pubsub, &out, 1);
        }

        // ── cyclone service: nano server ↔ ros2 service call ─────────────
        Scenario::CycloneServiceNanoServer => {
            let domain = nros_tests::unique_ros_domain_id();
            let server_bin = nano_cyclone_c_binary("service-server", "c_service_server");
            let mut server = spawn_nano_cyclone(&server_bin, "nano-cyclone-service-server", domain);
            // Services need queryable/endpoint discovery before the client call.
            std::thread::sleep(Duration::from_secs(4));
            let mut client = Ros2DdsProcess::service_call_cyclonedds_with_domain(
                SRV,
                SRV_TYPE,
                "{a: 5, b: 3}",
                DEFAULT_ROS_DISTRO,
                domain,
            )
            .expect("start ros2 cyclone service call");

            let out = client
                .wait_for_output(Duration::from_secs(10))
                .unwrap_or_default();
            client.kill();
            server.kill();

            assert!(
                out.contains("sum=8") || out.contains("response"),
                "ROS 2 cyclone client did not get the AddTwoInts reply (expected sum=8) \
                 ({}).\n{out}",
                cell.note
            );
        }

        // ── zenoh lifecycle: drive the REP-2002 surface via ros2 lifecycle ─
        Scenario::ZenohLifecycle => {
            let router = start_zenoh_router();
            let locator = router.locator();

            let mut node = spawn_nano_zenoh(&lifecycle_node_binary(), "lifecycle-node", &locator);
            let boot_log = node
                .wait_for_output_pattern("Ready. Drive the lifecycle", Duration::from_secs(15))
                .expect("lifecycle-node never reached ready state");
            assert!(
                boot_log.contains("Lifecycle services registered"),
                "boot log missing service-registration marker: {boot_log}"
            );

            // A: `ros2 lifecycle nodes` discovers /lifecycle_demo.
            let nodes = poll_ros2_until(
                &locator,
                "lifecycle nodes --no-daemon --spin-time 0.1",
                "/lifecycle_demo",
                Duration::from_secs(10),
            );
            assert!(
                nodes.contains("/lifecycle_demo"),
                "ros2 lifecycle nodes did not list /lifecycle_demo ({}):\n{nodes}",
                cell.note
            );

            // B: initial get returns unconfigured.
            let before = run_ros2(
                &locator,
                "lifecycle get --no-daemon --spin-time 0.1 /lifecycle_demo",
            );
            assert!(
                before.to_lowercase().contains("unconfigured"),
                "expected Unconfigured before configure, got:\n{before}"
            );

            // C: set configure → inactive + fires on_configure.
            let configure = run_ros2(
                &locator,
                "lifecycle set --no-daemon --spin-time 0.1 /lifecycle_demo configure",
            );
            assert!(
                configure.contains("Transitioning successful"),
                "configure did not report success:\n{configure}"
            );
            node.wait_for_output_pattern("on_configure", Duration::from_secs(3))
                .expect("on_configure never logged");
            let after = poll_ros2_until(
                &locator,
                "lifecycle get --no-daemon --spin-time 0.1 /lifecycle_demo",
                "inactive",
                Duration::from_secs(5),
            );
            assert!(
                after.to_lowercase().contains("inactive"),
                "expected Inactive after configure, got:\n{after}"
            );

            // D: list shows reachable transitions from Inactive.
            let list = run_ros2(
                &locator,
                "lifecycle list --no-daemon --spin-time 0.1 /lifecycle_demo",
            );
            for marker in ["activate", "cleanup", "shutdown"] {
                assert!(
                    list.contains(marker),
                    "ros2 lifecycle list missing `{marker}`:\n{list}"
                );
            }
            node.kill();
        }
    }
}

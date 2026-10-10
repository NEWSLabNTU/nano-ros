//! Phase 209 C++ port templates — the acceptance, executed.
//!
//! **Bucket: matrix consumer.** The `Workload::Port` cells of `matrix::CELLS`
//! (phase-482 W3), one per platform: posix, FreeRTOS (mps2-an385) and Zephyr
//! (mps2/an385). What they prove is a PORTING property rather than delivery: a
//! stock ROS 2 C++ program, vendored verbatim with its `main`, builds and runs
//! against nano-ros with only build glue changed. Until W3 this file said there
//! was no cell axis for that; `Workload::Port` is the axis.
//!
//! The source is shared. `src/minimal_publisher.cpp` is compiled by the
//! template's stock ament `CMakeLists.txt` on posix, and by the two small build
//! directories beside it on the RTOSes (`mps2-an385-freertos/`, `zephyr/`),
//! each of which is the whole port for its platform.
//!
//! # Why this file exists
//!
//! Issue 0469. Phase 209's three port templates were in NO lane — no fixture
//! row, no test, no recipe — for over two months. Nothing built or ran them
//! between 2026-05-30 and 2026-08-07, and in that window the acceptance
//! silently stopped holding: issue 0465, the rclcpp shim opening a second RMW
//! session, so the node died at startup with `Transport(ConnectionFailed)`.
//!
//! The shape of that failure decides the shape of this test. The template
//! **compiled and linked cleanly the entire time it was broken** — so a
//! build-only fixture row would have stayed green throughout and taught us
//! nothing. The acceptance is "compiles + links + RUNS"; only the third part
//! was lost, so the third part is what must be asserted here.
//!
//! The binaries come from `examples/fixtures.toml`
//! (`cpp_port_*`, builder `cmake-configure`) — tests never compile
//! (AGENTS.md Testing).

use nros_tests::{
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, Rmw, ZenohRouter, build_cmake_leaf_rmw,
        build_contract_monitor_diagsink, build_int32_sink, build_int32_source,
        build_zephyr_cortex_m_leaf, is_qemu_available, require_zenohd, zenohd_unique,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{
        CONTRACT_MONITOR_DIAGSINK_READY_MARKER, CPP_PORT_MONITOR_HARDWARE_ID,
        CPP_PORT_MONITOR_TOPICS, CPP_PORT_MONITOR_UP_MARKER, CPP_PORT_PUBLISH_MARKER,
        CPP_PORT_SMOKE_DIAG_TASK, CPP_PORT_SMOKE_HARDWARE_ID, CPP_PORT_SMOKE_TOPIC,
        CPP_PORT_SMOKE_UP_MARKER, DIAG_LEVEL_ERROR, DIAG_LEVEL_OK, INT32_LISTENER_LOG_PREFIX,
        diagsink_status_line,
    },
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The canonical ROS 2 "minimal publisher", vendored verbatim, publishes over
/// nano-ros.
///
/// This is phase 209's headline claim, and the one that rotted. Asserting the
/// marker rather than merely "the process stayed up" matters: under issue 0465
/// the process also exited, but a shim that opens a session and then publishes
/// nothing would satisfy a liveness check while failing the actual promise.
#[rstest]
fn cpp_port_minimal_publisher_publishes(zenohd_unique: ZenohRouter) {
    require_zenohd();

    let bin = nros_tests::fixtures::require_cmake_fixture(
        "cpp_port_minimal_publisher",
        "minimal_publisher",
    )
    .expect("phase-209 port template fixture");

    let mut cmd = std::process::Command::new(bin);
    cmd.env("NROS_LOCATOR", zenohd_unique.locator());
    let mut node = ManagedProcess::spawn_command(cmd, "cpp-port-minimal-publisher")
        .expect("spawn the ported node");

    // The template logs through the rclcpp compat surface's `RCLCPP_INFO`, so a
    // failure here is either "it never got a session" (0465's shape) or "the log
    // macro lost the line" — both worth failing on.
    // `wait_for_output_pattern` returns `Ok(output)` on TIMEOUT too, as long as
    // the process printed anything at all — it is "collect output, stopping
    // early if the pattern shows up", not an assertion. Checking only the
    // `Result` is how this test first passed against a deliberately broken
    // fixture: the failing node's `Transport(InvalidConfig)` line is non-empty
    // output, so the call returned `Ok`. Assert on the CONTENT.
    let out = node.collect_until(CPP_PORT_PUBLISH_MARKER, Duration::from_secs(20));
    assert!(
        out.contains(CPP_PORT_PUBLISH_MARKER),
        "the vendored ROS 2 tutorial node did not publish through nano-ros \
         (expected a line containing `{CPP_PORT_PUBLISH_MARKER}`).\n\
         Phase 209's acceptance is that this source builds AND RUNS unmodified; \
         issue 0465 was exactly this symptom, from the rclcpp shim opening a \
         second RMW session on a one-entry pool.\n\
         --- node output ---\n{out}"
    );
}

/// The template directory every platform's build of the ported node shares.
const MINIMAL_PUBLISHER: &str = "examples/templates/cpp-port-minimal-publisher";

/// Boot + session + first 500 ms timer tick, on the same QEMU machine the
/// FreeRTOS and Zephyr Cortex-M pubsub cells use (they measured publishing in
/// under 3 s of guest time; 30 s leaves room for a loaded host).
const RTOS_PUBLISH_BUDGET: Duration = Duration::from_secs(30);

/// Assert the RTOS guest's console shows the tutorial's publish line, with the
/// failure text naming what an RTOS port typically gets wrong.
fn assert_ported_publish(platform: &str, out: &str) {
    assert!(
        out.contains(CPP_PORT_PUBLISH_MARKER),
        "the unmodified ROS 2 tutorial publisher did not publish on {platform} \
         (expected a line containing `{CPP_PORT_PUBLISH_MARKER}`).\n\
         An `abort()` after `create_publisher(\"topic\") failed` with \
         `ConnectionFailed` means the ported TU never saw the locator the board \
         bakes; no output past boot means the board startup never reached the \
         ported `main` (ROS2_MAIN's `nros_app_main` forwarder).\n\
         --- guest output ---\n{out}"
    );
}

/// phase-482 W3 — the same tutorial source on FreeRTOS (mps2-an385, QEMU).
///
/// The board bakes its locator (a Cortex-M image has no environment), so the
/// router port is the cell's allocator slot and must match the fixture row's
/// `NROS_ENTRY_LOCATOR`.
#[test]
fn cpp_port_minimal_publisher_publishes_on_freertos() {
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    require_zenohd();
    let bin = build_cmake_leaf_rmw(
        &format!("{MINIMAL_PUBLISHER}/mps2-an385-freertos"),
        "minimal_publisher",
        Rmw::Zenoh,
    )
    .require("FreeRTOS port of the minimal publisher");

    let port = port_of(PlatformId::FreertosMps2, Lang::Cpp, Workload::Port);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port}: {e:?}"));
    let mut qemu =
        QemuProcess::start_mps2_an385_freertos_slirp(&bin).expect("spawn the FreeRTOS port");
    let out = qemu.collect_until(CPP_PORT_PUBLISH_MARKER, RTOS_PUBLISH_BUDGET);
    qemu.kill();
    assert_ported_publish("FreeRTOS mps2-an385", &out);
}

/// phase-482 W3 — the same tutorial source on Zephyr (mps2/an385, QEMU).
///
/// Not native_sim: a ported program needs the full libstdc++, and native_sim's
/// C library cannot carry the host's (phase-209 G.2). This board runs Zephyr's
/// own IP stack through SLIRP, so the router listens on 0.0.0.0 and the image
/// dials 10.0.2.2.
#[test]
fn cpp_port_minimal_publisher_publishes_on_zephyr() {
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    require_zenohd();
    let bin = build_zephyr_cortex_m_leaf(
        &format!("{MINIMAL_PUBLISHER}/zephyr"),
        "build-cortex-m-cpp-port-minimal-publisher-zenoh",
        "cpp",
        Rmw::Zenoh,
    )
    .require("Zephyr port of the minimal publisher");

    let port = port_of(PlatformId::ZephyrQemuCortexM, Lang::Cpp, Workload::Port);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port}: {e:?}"));
    let mut qemu = QemuProcess::start_mps2_an385_networked(&bin).expect("spawn the Zephyr port");
    let out = qemu.collect_until(CPP_PORT_PUBLISH_MARKER, RTOS_PUBLISH_BUDGET);
    qemu.kill();
    assert_ported_publish("Zephyr mps2/an385", &out);
}

// ---------------------------------------------------------------------------
// phase-482 W3 — the other two port templates, `rclcpp-compat-smoke`
// (`Workload::PortSmoke`) and `topic-state-monitor-port`
// (`Workload::PortMonitor`), each on posix, FreeRTOS and Zephyr.
//
// Unlike the tutorial publisher, both are asserted from OUTSIDE: what they
// promise is traffic, so a host peer has to receive (or feed) it. The peers
// are host fixtures on the same router — `int32-sink`, `int32-source` and the
// contract-monitor diagsink — so every cell is nano-ros end to end and needs
// no ROS 2 install, only the router.
// ---------------------------------------------------------------------------

const SMOKE: &str = "examples/templates/rclcpp-compat-smoke";
const MONITOR: &str = "examples/templates/topic-state-monitor-port";

/// Boot + session + several 100 ms ticks + one 1 s diagnostics period, on a
/// QEMU guest or a host process.
const PORT_TRAFFIC_BUDGET: Duration = Duration::from_secs(40);

/// How long the diagsink observer lives (`CM_RUN_MS`). Longer than any one
/// cell, which kills it explicitly.
const DIAGSINK_LIFETIME_MS: &str = "120000";

/// The ported program under test: a host process (posix) or a QEMU guest.
enum Guest {
    Host(ManagedProcess),
    Qemu(QemuProcess),
}

impl Guest {
    fn collect_until(&mut self, pattern: &str, timeout: Duration) -> String {
        match self {
            Guest::Host(p) => p.collect_until(pattern, timeout),
            Guest::Qemu(q) => q.collect_until(pattern, timeout),
        }
    }

    fn kill(&mut self) {
        match self {
            Guest::Host(p) => p.kill(),
            Guest::Qemu(q) => q.kill(),
        }
    }
}

/// Where a port cell runs, and how its router is reached.
#[derive(Clone, Copy)]
enum PortPlatform {
    Linux,
    Freertos,
    Zephyr,
}

impl PortPlatform {
    fn label(self) -> &'static str {
        match self {
            PortPlatform::Linux => "posix",
            PortPlatform::Freertos => "FreeRTOS mps2-an385",
            PortPlatform::Zephyr => "Zephyr mps2/an385",
        }
    }

    fn id(self) -> PlatformId {
        match self {
            PortPlatform::Linux => PlatformId::Linux,
            PortPlatform::Freertos => PlatformId::FreertosMps2,
            PortPlatform::Zephyr => PlatformId::ZephyrQemuCortexM,
        }
    }

    /// Resolve the platform's build of `template`: the template's own ament
    /// build on posix (`compile_check_id`), its `mps2-an385-freertos/` or
    /// `zephyr/` sub-project on an RTOS (the latter's west build directory is
    /// `west_build`).
    fn binary(
        self,
        template: &str,
        compile_check_id: &str,
        west_build: &str,
        exe: &str,
    ) -> PathBuf {
        match self {
            PortPlatform::Linux => {
                nros_tests::fixtures::require_cmake_fixture(compile_check_id, exe)
                    .unwrap_or_else(|e| panic!("posix build of {template} did not resolve: {e:?}"))
            }
            PortPlatform::Freertos => {
                build_cmake_leaf_rmw(&format!("{template}/mps2-an385-freertos"), exe, Rmw::Zenoh)
                    .require("FreeRTOS port")
                    .to_path_buf()
            }
            PortPlatform::Zephyr => build_zephyr_cortex_m_leaf(
                &format!("{template}/zephyr"),
                west_build,
                "cpp",
                Rmw::Zenoh,
            )
            .require("Zephyr port")
            .to_path_buf(),
        }
    }

    /// Start a router this platform's guest can reach. Posix uses an
    /// ephemeral loopback port; an RTOS guest dials the port its fixture row
    /// BAKED (`alloc::port_of`), through SLIRP, so the router listens there on
    /// every interface. Host peers always use `locator()`.
    fn router(self, workload: Workload) -> ZenohRouter {
        match self {
            PortPlatform::Linux => ZenohRouter::start_unique()
                .unwrap_or_else(|e| panic!("failed to start zenohd: {e:?}")),
            _ => {
                let port = port_of(self.id(), Lang::Cpp, workload);
                ZenohRouter::start_slirp(port)
                    .unwrap_or_else(|e| panic!("failed to start zenohd on {port}: {e:?}"))
            }
        }
    }

    fn launch(self, bin: &Path, locator: &str, name: &str) -> Guest {
        match self {
            PortPlatform::Linux => {
                let mut cmd = std::process::Command::new(bin);
                cmd.env("NROS_LOCATOR", locator);
                Guest::Host(
                    ManagedProcess::spawn_command(cmd, name).expect("spawn the ported node"),
                )
            }
            PortPlatform::Freertos => Guest::Qemu(
                QemuProcess::start_mps2_an385_freertos_slirp(bin).expect("spawn the FreeRTOS port"),
            ),
            PortPlatform::Zephyr => Guest::Qemu(
                QemuProcess::start_mps2_an385_networked(bin).expect("spawn the Zephyr port"),
            ),
        }
    }

    /// The host preconditions, checked AFTER fixture resolution so an
    /// out-of-lane cell lane-skips instead of failing on a missing emulator.
    fn require_host(self) {
        if !matches!(self, PortPlatform::Linux) && !is_qemu_available() {
            nros_tests::unmet!("qemu-system-arm not found");
        }
        require_zenohd();
    }
}

/// Spawn a host peer fixture on `locator` with extra environment.
fn spawn_peer(bin: &Path, name: &str, locator: &str, envs: &[(&str, &str)]) -> ManagedProcess {
    let mut cmd = std::process::Command::new(bin);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    ManagedProcess::spawn_command(cmd, name.to_string()).expect("spawn a host peer fixture")
}

/// Read `peer` until every one of `patterns` has appeared, or `budget`
/// passes. Returns the whole transcript read, so the caller asserts on it.
///
/// `collect_until` stops at ONE pattern and returns only what that call read;
/// the diagnostics arrive as one line per task, often in one read, so the
/// patterns are awaited in turn and the reads concatenated.
fn read_until_all(peer: &mut ManagedProcess, patterns: &[String], budget: Duration) -> String {
    let deadline = Instant::now() + budget;
    let mut seen = String::new();
    while let Some(missing) = patterns.iter().find(|p| !seen.contains(p.as_str())) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        seen.push_str(&peer.collect_until(missing, left));
    }
    seen
}

/// `rclcpp-compat-smoke` on `platform`: its Int32 topic reaches a host
/// `int32-sink`, and its `diagnostic_updater` task reaches the diagsink.
fn run_port_smoke(platform: PortPlatform) {
    let bin = platform.binary(
        SMOKE,
        "cpp_port_rclcpp_compat_smoke",
        "build-cortex-m-cpp-port-rclcpp-compat-smoke-zenoh",
        "rclcpp_compat_smoke",
    );
    let sink_bin = build_int32_sink().require("int32-sink");
    let diagsink_bin = build_contract_monitor_diagsink().require("contract-monitor-diagsink");
    platform.require_host();

    let router = platform.router(Workload::PortSmoke);
    let locator = router.locator();
    let mut sink = spawn_peer(
        sink_bin,
        "port-smoke-int32-sink",
        &locator,
        &[("NROS_SUB_TOPIC", CPP_PORT_SMOKE_TOPIC)],
    );
    let mut diagsink = spawn_peer(
        diagsink_bin,
        "port-smoke-diagsink",
        &locator,
        &[("CM_RUN_MS", DIAGSINK_LIFETIME_MS)],
    );
    diagsink
        .wait_for_output_pattern(
            CONTRACT_MONITOR_DIAGSINK_READY_MARKER,
            Duration::from_secs(10),
        )
        .expect("diagsink did not become ready");

    let mut guest = platform.launch(&bin, &locator, "rclcpp-compat-smoke");
    let boot = guest.collect_until(CPP_PORT_SMOKE_UP_MARKER, PORT_TRAFFIC_BUDGET);
    assert!(
        boot.contains(CPP_PORT_SMOKE_UP_MARKER),
        "the unmodified rclcpp-compat-smoke never reached its node constructor on {} \
         (expected `{CPP_PORT_SMOKE_UP_MARKER}`).\n--- guest output ---\n{boot}",
        platform.label()
    );

    let (received, why) =
        sink.collect_until_count(INT32_LISTENER_LOG_PREFIX, 3, PORT_TRAFFIC_BUDGET);
    let status = diagsink_status_line(
        CPP_PORT_SMOKE_DIAG_TASK,
        CPP_PORT_SMOKE_HARDWARE_ID,
        DIAG_LEVEL_OK,
    );
    let diags = read_until_all(
        &mut diagsink,
        std::slice::from_ref(&status),
        PORT_TRAFFIC_BUDGET,
    );
    let tail = guest.collect_until(CPP_PORT_SMOKE_UP_MARKER, Duration::from_secs(1));
    guest.kill();
    sink.kill();
    diagsink.kill();

    assert!(
        received.matches(INT32_LISTENER_LOG_PREFIX).count() >= 3,
        "a host int32-sink on {CPP_PORT_SMOKE_TOPIC} did not receive 3 samples from the \
         ported rclcpp-compat-smoke on {}. The node started (its constructor logged), so \
         the publish loop (`rclcpp::spin_some` + `std::this_thread::sleep_for`) or the \
         wire is what failed.\n--- sink ---\n{received}{}\n--- guest ---\n{boot}{tail}",
        platform.label(),
        why.unwrap_or_default()
    );
    assert!(
        diags.contains(&status),
        "the ported node's diagnostic_updater task never reached /diagnostics on {} \
         (expected `{status}`). `Updater::update()` rate-limits on \
         std::chrono::steady_clock, so a clock that never advances publishes nothing.\n\
         --- diagsink ---\n{diags}\n--- guest ---\n{boot}{tail}",
        platform.label()
    );
}

/// `topic-state-monitor-port` on `platform`: each watched topic is reported
/// STALE while nothing publishes it, then LIVE once a host `int32-source`
/// publishes both.
///
/// The order is the assertion. A monitor whose clock never advances computes
/// an age of zero and reports every topic live with no peer at all, so "live"
/// alone proves nothing; "stale, then live after the peer starts" proves both
/// the clock and the capturing-lambda subscriptions.
fn run_port_monitor(platform: PortPlatform) {
    let bin = platform.binary(
        MONITOR,
        "cpp_port_topic_state_monitor",
        "build-cortex-m-cpp-port-topic-state-monitor-zenoh",
        "topic_state_monitor",
    );
    let source_bin = build_int32_source().require("int32-source");
    let diagsink_bin = build_contract_monitor_diagsink().require("contract-monitor-diagsink");
    platform.require_host();

    let router = platform.router(Workload::PortMonitor);
    let locator = router.locator();
    let mut diagsink = spawn_peer(
        diagsink_bin,
        "port-monitor-diagsink",
        &locator,
        &[("CM_RUN_MS", DIAGSINK_LIFETIME_MS)],
    );
    diagsink
        .wait_for_output_pattern(
            CONTRACT_MONITOR_DIAGSINK_READY_MARKER,
            Duration::from_secs(10),
        )
        .expect("diagsink did not become ready");

    let mut guest = platform.launch(&bin, &locator, "topic-state-monitor");
    let boot = guest.collect_until(CPP_PORT_MONITOR_UP_MARKER, PORT_TRAFFIC_BUDGET);
    assert!(
        boot.contains(CPP_PORT_MONITOR_UP_MARKER),
        "the unmodified topic_state_monitor never finished setting up on {} \
         (expected `{CPP_PORT_MONITOR_UP_MARKER}`).\n--- guest output ---\n{boot}",
        platform.label()
    );

    let lines = |level: u8| -> Vec<String> {
        CPP_PORT_MONITOR_TOPICS
            .iter()
            .map(|t| diagsink_status_line(t, CPP_PORT_MONITOR_HARDWARE_ID, level))
            .collect()
    };
    let stale = lines(DIAG_LEVEL_ERROR);
    let before = read_until_all(&mut diagsink, &stale, PORT_TRAFFIC_BUDGET);
    let stale_seen = stale.iter().all(|l| before.contains(l.as_str()));

    // Only once both topics read stale does anything publish them.
    let topics = CPP_PORT_MONITOR_TOPICS.map(|t| format!("/{t}")).join(",");
    let mut source = stale_seen.then(|| {
        spawn_peer(
            source_bin,
            "port-monitor-int32-source",
            &locator,
            &[("NROS_PUB_TOPICS", &topics), ("NROS_PUB_PERIOD_MS", "100")],
        )
    });
    let live = lines(DIAG_LEVEL_OK);
    let after = if stale_seen {
        read_until_all(&mut diagsink, &live, PORT_TRAFFIC_BUDGET)
    } else {
        String::new()
    };
    let tail = guest.collect_until(CPP_PORT_MONITOR_UP_MARKER, Duration::from_secs(1));
    guest.kill();
    diagsink.kill();
    if let Some(s) = source.as_mut() {
        s.kill();
    }

    assert!(
        stale_seen,
        "with no publisher, the ported monitor on {} never reported both topics stale \
         (expected {stale:?}). Its age is `steady_clock::now() - last_seen`, so a clock \
         that never advances reports `live` forever.\n--- diagsink ---\n{before}\n\
         --- guest ---\n{boot}{tail}",
        platform.label()
    );
    assert!(
        live.iter().all(|l| after.contains(l.as_str())),
        "after a host int32-source started publishing {topics}, the ported monitor on {} \
         did not report both live (expected {live:?}). The capturing-lambda subscriptions \
         never ran, or never ran from `rclcpp::spin_some`.\n--- diagsink ---\n{after}\n\
         --- guest ---\n{boot}{tail}",
        platform.label()
    );
}

/// phase-482 W3 — `rclcpp-compat-smoke`, unmodified, on posix.
#[test]
fn cpp_port_rclcpp_compat_smoke_publishes() {
    run_port_smoke(PortPlatform::Linux);
}

/// phase-482 W3 — `rclcpp-compat-smoke`, unmodified, on FreeRTOS (mps2-an385).
#[test]
fn cpp_port_rclcpp_compat_smoke_publishes_on_freertos() {
    run_port_smoke(PortPlatform::Freertos);
}

/// phase-482 W3 — `rclcpp-compat-smoke`, unmodified, on Zephyr (mps2/an385).
#[test]
fn cpp_port_rclcpp_compat_smoke_publishes_on_zephyr() {
    run_port_smoke(PortPlatform::Zephyr);
}

/// phase-482 W3 — `topic-state-monitor-port`, unmodified, on posix.
#[test]
fn cpp_port_topic_state_monitor_reports_liveness() {
    run_port_monitor(PortPlatform::Linux);
}

/// phase-482 W3 — `topic-state-monitor-port`, unmodified, on FreeRTOS.
#[test]
fn cpp_port_topic_state_monitor_reports_liveness_on_freertos() {
    run_port_monitor(PortPlatform::Freertos);
}

/// phase-482 W3 — `topic-state-monitor-port`, unmodified, on Zephyr.
#[test]
fn cpp_port_topic_state_monitor_reports_liveness_on_zephyr() {
    run_port_monitor(PortPlatform::Zephyr);
}

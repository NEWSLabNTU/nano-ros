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
        build_zephyr_cortex_m_leaf, is_qemu_available, require_zenohd, zenohd_unique,
    },
    matrix::{Lang, PlatformId, Workload},
    output::CPP_PORT_PUBLISH_MARKER,
};
use rstest::rstest;
use std::time::Duration;

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
        nros_tests::skip!("qemu-system-arm not found");
    }
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
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
        nros_tests::skip!("qemu-system-arm not found");
    }
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
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

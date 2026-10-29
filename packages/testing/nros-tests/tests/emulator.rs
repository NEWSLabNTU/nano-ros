//! Emulator tests for nros
//!
//! **Bucket (phase-329 W4): KEEP — sole `baremetal` coverage + on-target
//! one-offs, not matrix-cell dups.** rtos_e2e's `Platform` enum is RTOS-only
//! (Freertos/Nuttx/ThreadxLinux/ThreadxRiscv64); this file is the ONLY runtime
//! coverage of the BARE-METAL Cortex-M3 (MPS2-AN385) target, so nothing duplicates
//! it. Two kinds live here, both kept:
//!   * ON-TARGET UNIT tests no cell asserts — CDR serialization, Node API, type
//!     metadata, output format, the WCET benchmark, the LAN9118 driver, and the
//!     toolchain/qemu detection probes.
//!   * TRANSPORT/FRAMEWORK pubsub VARIANTS on `baremetal` — `bsp` (the
//!     standard), `serial` (serial transport), `xrce` (XRCE rmw), `rtic` (the RTIC
//!     framework entry shape). These overlap the `baremetal` matrix
//!     coordinate in intent but there is no cell-bound baremetal consumer to fold
//!     into; when one exists, the `bsp` case is the fold candidate and the
//!     serial/xrce/rtic variants stay one-offs.
//!
//! No deletions: sole platform coverage, and QEMU folds can't be run-proven for a
//! safe delete on a host without exercising every variant.
//!
//! Tests that run on QEMU Cortex-M3 emulator without physical hardware.
//! These verify CDR serialization, Node API, and type metadata work on embedded targets.
//!
//! Run with: `cargo test -p nano-ros-tests --test emulator -- --nocapture`
//! Or: `just test-rust-emulator`
//!
//! ## BSP Tests
//!
//! The BSP (Board Support Package) tests verify the simplified nros-board-mps2-an385 API:
//! - `just test-qemu-bsp` - Run BSP build and startup tests

use nros_tests::{
    assert_output_contains, assert_output_excludes, count_pattern,
    fixtures::{
        QemuProcess, RequireFixture, SocatPtyPair, XrceSerialAgent, ZenohRouter,
        build_qemu_bsp_listener, build_qemu_bsp_talker, build_qemu_lan9118,
        build_qemu_rtic_action_client, build_qemu_rtic_action_server, build_qemu_rtic_listener,
        build_qemu_rtic_mixed_listener, build_qemu_rtic_mixed_talker,
        build_qemu_rtic_service_client, build_qemu_rtic_service_server, build_qemu_rtic_talker,
        build_qemu_serial_listener, build_qemu_serial_talker, build_qemu_talker_xrce,
        build_qemu_wcet_bench, is_arm_toolchain_available, is_qemu_available, is_socat_available,
        or_unmet, parse_test_results, qemu_baremetal_capi_binary, require_xrce_agent,
        require_zenoh_pico_arm,
    },
    matrix::{Lang, PlatformId, Workload},
    platform, wait_for_port,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

/// Issue 1900 — every test here is a bare-metal QEMU image, so the lane is
/// asked FIRST: a lane that selects no such coordinate deselects the test
/// before any host probe, whatever the host has. Every helper below calls it,
/// so the first helper a test reaches is lane-first whichever it is.
fn require_baremetal_in_lane() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::QemuBaremetal],
        "a bare-metal QEMU image",
    );
}

/// The `qemu-test` image, resolved only after the lane check. An rstest
/// fixture runs BEFORE the test body, so the body's own `require_*` cannot
/// order it.
#[rstest::fixture]
fn qemu_binary() -> PathBuf {
    require_baremetal_in_lane();
    nros_tests::fixtures::build_qemu_test()
        .require("qemu-test")
        .to_path_buf()
}

/// Fail (unmet) if QEMU is not available — after the lane check.
fn require_qemu() {
    require_baremetal_in_lane();
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
}

/// Fail (unmet) if the ARM toolchain is not available — after the lane check.
fn require_arm_toolchain() {
    require_baremetal_in_lane();
    if !is_arm_toolchain_available() {
        nros_tests::unmet!("thumbv7m-none-eabi target not installed");
    }
}

// =============================================================================
// QEMU Cortex-M3 Tests
// =============================================================================

#[rstest]
fn test_qemu_cdr_serialization(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify CDR serialization tests passed
    assert_output_contains(
        &output,
        &[
            "[PASS] Int32 roundtrip",
            "[PASS] Float64 roundtrip",
            "[PASS] Time roundtrip",
            "[PASS] CDR header",
        ],
    );

    // Verify no test failures
    assert_output_excludes(&output, &["[FAIL]"]);
}

#[rstest]
fn test_qemu_node_api(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify Node API tests passed
    assert_output_contains(
        &output,
        &[
            "[PASS] Node creation",
            "[PASS] Node publisher",
            "[PASS] Node subscriber",
            "[PASS] Node serialize",
        ],
    );
}

#[rstest]
fn test_qemu_type_metadata(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify type metadata test passed
    assert_output_contains(&output, &["[PASS] Type names"]);
}

#[rstest]
fn test_qemu_all_tests_pass(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Parse and verify results
    let (passed, failed) = parse_test_results(&output);

    assert!(
        passed >= 9,
        "Expected at least 9 tests to pass, got {}",
        passed
    );
    assert_eq!(
        failed, 0,
        "Expected no failures, got {}. Output:\n{}",
        failed, output
    );

    // Verify completion message
    assert_output_contains(&output, &["All tests passed"]);
}

#[rstest]
fn test_qemu_output_format(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify output has expected format
    let pass_count = count_pattern(&output, "[PASS]");
    let fail_count = count_pattern(&output, "[FAIL]");

    eprintln!("Test results: {} passed, {} failed", pass_count, fail_count);
    eprintln!("Output:\n{}", output);

    assert!(pass_count > 0, "No [PASS] markers found in output");
}

// =============================================================================
// QEMU WCET Benchmark
// =============================================================================

/// Boot the WCET bench on the emulated Cortex-M3 and return its console.
fn run_qemu_wcet_bench() -> String {
    require_qemu();
    require_arm_toolchain();

    let binary = build_qemu_wcet_bench().require("qemu-wcet-bench");

    let mut qemu = QemuProcess::start_cortex_m3(binary).expect("Failed to start QEMU");

    qemu.wait_for_output(Duration::from_secs(60))
        .expect("QEMU timed out")
}

/// What the bench MUST do where it cannot measure: refuse, with no numbers.
///
/// Issue 0403 decided that a bench whose cycle counter does not count emits
/// NO measurements and fails, rather than reporting zeros — a zero reads as
/// "this operation is free", the most optimistic WCET there is, and consuming
/// it always errs toward "schedulable". QEMU does not implement DWT cycle
/// counting, so on this emulator the refusal is the contract under test.
///
/// This used to be one test that asserted the completion line and turned the
/// refusal into `unmet!`, so every QEMU lane counted the bench working as
/// designed as a real failure (issue 1991, row 13). Now the refusal is
/// asserted: a bench that starts printing a table of zeros here goes red.
#[test]
fn qemu_wcet_bench_refuses_without_a_cycle_counter() {
    let output = run_qemu_wcet_bench();

    assert!(
        output.contains(nros_tests::output::WCET_DEAD_COUNTER_MARKER),
        "the WCET bench did not report a dead cycle counter on QEMU. Either it \
         measured (QEMU now counts DWT cycles, so this test's premise has moved: \
         enable `wcet_bench_completes_with_a_counting_cycle_counter`) or it \
         failed some other way. Output:\n{output}"
    );
    assert_output_excludes(
        &output,
        &[
            nros_tests::output::WCET_MEASUREMENT_MARKER,
            nros_tests::output::WCET_COMPLETE_MARKER,
            "[PASS]",
        ],
    );
    assert_output_contains(&output, &["[FAIL]"]);
}

/// What the bench must do where it CAN measure: complete, with no failure.
///
/// Needs a core whose DWT cycle counter counts, which QEMU's is not, and no
/// lane runs the bench anywhere else yet — so this is off until one does
/// (issue 0403). It boots the same QEMU image as the refusal test, so it is
/// also the test to enable if QEMU ever gains DWT counting.
#[test]
#[ignore = "needs a DWT cycle counter that counts; QEMU has none and no hardware lane exists (issue 0403, issue 1991)"]
fn wcet_bench_completes_with_a_counting_cycle_counter() {
    let output = run_qemu_wcet_bench();

    assert_output_excludes(&output, &[nros_tests::output::WCET_DEAD_COUNTER_MARKER]);
    // Note: [PASS] is printed after the completion marker but wait_for_output
    // kills the process on that marker, so we may not capture [PASS].
    assert_output_contains(&output, &[nros_tests::output::WCET_COMPLETE_MARKER]);
    assert_output_excludes(&output, &["[FAIL]"]);
}

// =============================================================================
// QEMU LAN9118 Driver Test
// =============================================================================

#[test]
fn test_qemu_lan9118_driver() {
    require_qemu();
    require_arm_toolchain();

    let binary = build_qemu_lan9118().require("qemu-lan9118");

    let mut qemu = QemuProcess::start_mps2_an385(binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    let (passed, failed) = parse_test_results(&output);

    assert!(
        passed >= 5,
        "Expected at least 5 tests to pass, got {}. Output:\n{}",
        passed,
        output
    );
    assert_eq!(
        failed, 0,
        "Expected no failures, got {}. Output:\n{}",
        failed, output
    );
    assert_output_contains(&output, &["All tests passed"]);
}

// =============================================================================
// QEMU Availability Tests
// =============================================================================
//
// ~~`test_qemu_detection`~~ / ~~`test_arm_toolchain_detection`~~ removed: each read one
// `is_*_available()` boolean and printed it, asserting nothing, so both passed
// on a host with neither installed ("this test just verifies the detection
// works" — but a detection that returns `false` also "works"). The identical
// ~~`test_arm_toolchain_detection`~~ lived in `platform.rs` too. Both probes stay
// where they can actually fail: the `unmet!` guards on the real QEMU tests
// above. Forbidden repo-wide by `check-no-vacuous-tests`.

// =============================================================================
// QEMU BSP Tests (Phase 17.7)
// =============================================================================
//
// Tests for the simplified nros-board-mps2-an385 API (Board Support Package).
// These examples use a higher-level API than the rs-* examples.

// (Phase 182.3) The qemu-bsp `*_builds` presence tests were
// removed — they only asserted a fixture compiled, covered by `build-all`
// (`baremetal` is a manifest row, Phase 181.4/181.6) + the
// `_require-fixtures` preflight. (The BSP examples have no e2e here — they need
// Docker/slirp networking, see the skipped start tests below — so their compile
// coverage now lives solely in `build-all`, which is the test-all prerequisite.)

// =============================================================================
// BSP Network Tests (Require Docker or slirp networking)
// =============================================================================
//
// The BSP examples use the MPS2-AN385 machine with LAN9118 Ethernet.
// QEMU uses slirp (user-mode) networking: each instance gets an isolated
// 10.0.2.0/24 network. Firmware connects to zenohd via slirp gateway
// 10.0.2.2:<port>, which maps to host 127.0.0.1:<port>. No TAP bridge needed.
//
// To run BSP network tests:
//   just test-rust-qemu-baremetal-bsp  (uses Docker)
//
// Or manually:
//   ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
//   ./scripts/qemu/launch-mps2-an385.sh --binary <path>

/// BSP ethernet pub/sub e2e over QEMU slirp — no Docker, no TAP (Phase 203).
///
/// Both MPS2-AN385 instances run with `-nic user,model=lan9118` (slirp): each
/// gets an isolated `10.0.2.0/24`, but both reach the host zenohd via the slirp
/// gateway → the host, so zenohd is the rendezvous (the BSP example locator
/// is baked to the allocator's `BAREMETAL_BSP_PORT` aux slot — phase-295 W4:
/// the BSP pair owns its own router, so this test no longer serializes with
/// the RTIC / mixed-priority / large-msg lanes; the former
/// `qemu-baremetal-shared` group is retired). Replaces the former
/// `test_qemu_bsp_{talker,listener}_starts` blanket-skips with a real run;
/// gates cleanly (skip with reason) when the ARM toolchain / qemu /
/// zenoh-pico-arm / fixtures are absent.
#[test]
fn test_qemu_bsp_pubsub_e2e() {
    require_arm_toolchain();
    require_qemu();
    require_zenoh_pico_arm();

    let port = nros_tests::alloc::BAREMETAL_BSP_PORT; // the baked BSP locator port
    let talker_bin = build_qemu_bsp_talker().require("qemu-bsp-talker");
    let listener_bin = build_qemu_bsp_listener().require("qemu-bsp-listener");

    // zenohd (host) is the broker both slirp-isolated instances connect out to.
    eprintln!("Starting zenohd (slirp) on {port}...");
    let _zenohd = or_unmet(ZenohRouter::start_slirp(port));

    // Subscriber before publisher; brief settle so the listener is subscribed.
    eprintln!("Starting BSP listener QEMU...");
    let mut listener =
        QemuProcess::start_mps2_an385_networked(listener_bin).expect("Failed to start listener");
    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting BSP talker QEMU...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin).expect("Failed to start talker");

    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(60),
    );
    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );

    talker.kill();
    listener.kill();

    eprintln!("BSP listener output:\n{listener_output}");
    eprintln!("BSP talker output:\n{talker_output}");

    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    let published = count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX);
    eprintln!("BSP QEMU pubsub: published={published}, received={received}");

    assert!(published > 0, "BSP talker published 0 messages");
    assert!(received > 0, "BSP listener received 0 messages");
}

/// Issue 1512 — the C and C++ API on bare metal, end to end: every
/// `(QemuBaremetal, C|Cpp, Zenoh, *)` Runtime cell. The action pairs joined
/// with issue 1797 (a burst of batches desynchronised the smoltcp read).
///
/// Each pair is a cargo-rooted leaf under `examples/mps2-an385-baremetal/`:
/// the Rust startup owns the link and compiles the C/C++ application in. Both
/// images dial the locator their `system.toml` declares — `tcp/10.0.2.2:<port>`
/// with `<port>` = `alloc::port` of THIS cell, so the cases never share a
/// router — which reaches the C/C++ TU as `NROS_ENTRY_LOCATOR` and the boot
/// config as `Config::from_image`. Before both existed every C/C++ image
/// dialled the backend's own default (the guest's loopback) and the cells
/// were `BuildOnly`.
///
/// The first image is the one that must be up first (listener / server); the
/// marker is what proves the round trip, read from the image that observes it.
#[rstest]
#[case::c_pubsub(
    Lang::C,
    Workload::Pubsub,
    "baremetal-c-listener",
    "baremetal-c-talker"
)]
#[case::c_service(
    Lang::C,
    Workload::Service,
    "baremetal-c-service-server",
    "baremetal-c-service-client"
)]
#[case::cpp_pubsub(
    Lang::Cpp,
    Workload::Pubsub,
    "baremetal-cpp-listener",
    "baremetal-cpp-talker"
)]
#[case::cpp_service(
    Lang::Cpp,
    Workload::Service,
    "baremetal-cpp-service-server",
    "baremetal-cpp-service-client"
)]
#[case::c_action(
    Lang::C,
    Workload::Action,
    "baremetal-c-action-server",
    "baremetal-c-action-client"
)]
#[case::cpp_action(
    Lang::Cpp,
    Workload::Action,
    "baremetal-cpp-action-server",
    "baremetal-cpp-action-client"
)]
fn test_qemu_baremetal_capi_e2e(
    #[case] lang: Lang,
    #[case] workload: Workload,
    #[case] first: &str,
    #[case] second: &str,
) {
    require_arm_toolchain();
    require_qemu();

    let port = nros_tests::alloc::port_of(PlatformId::QemuBaremetal, lang, workload);
    let first_bin = qemu_baremetal_capi_binary(first).require(first);
    let second_bin = qemu_baremetal_capi_binary(second).require(second);

    let _zenohd = or_unmet(ZenohRouter::start_slirp(port));

    let mut a = QemuProcess::start_mps2_an385_networked(&first_bin)
        .unwrap_or_else(|e| panic!("start {first}: {e:?}"));
    std::thread::sleep(Duration::from_secs(5));
    let mut b = QemuProcess::start_mps2_an385_networked(&second_bin)
        .unwrap_or_else(|e| panic!("start {second}: {e:?}"));

    // Who observes the round trip: the listener hears the talker; the
    // CLIENT sees the reply (service sum / action result).
    let (marker, observer_is_first) = match workload {
        Workload::Pubsub => (nros_tests::output::LISTENER_LOG_PREFIX, true),
        Workload::Service => (nros_tests::output::SERVICE_RESULT_PREFIX, false),
        Workload::Action => (nros_tests::output::ACTION_RESULT_PREFIX, false),
        other => unreachable!("no bare-metal C/C++ case for {other:?}"),
    };
    let observed = if observer_is_first {
        a.collect_until(marker, Duration::from_secs(60))
    } else {
        b.collect_until(marker, Duration::from_secs(60))
    };
    // The other side's view is what tells "never reached" from "answered but
    // not heard" (issue 1797), so a failure prints both transcripts.
    let other_view = if observed.contains(marker) {
        String::new()
    } else if observer_is_first {
        b.collect_until("\u{0}", Duration::from_secs(1))
    } else {
        a.collect_until("\u{0}", Duration::from_secs(1))
    };
    b.kill();
    a.kill();
    let (observer, other) = if observer_is_first {
        (first, second)
    } else {
        (second, first)
    };
    eprintln!("{observer} output:\n{observed}");
    if !other_view.is_empty() {
        eprintln!("{other} output:\n{other_view}");
    }

    assert!(
        !observed.contains(nros_tests::output::SESSION_OPEN_FAILED_MARKER),
        "{observer} failed to open its session — check that its `[image.*] \
         locator` names port {port}, this cell's `alloc::port`"
    );
    assert!(
        observed.contains(marker),
        "{first} + {second} on port {port}: {observer} never printed `{marker}` \
         — the pair did not complete a round trip"
    );
}

// (Phase 182.3) ~~`test_qemu_bsp_both_build`~~ + `test_qemu_serial_{talker,listener}_builds`
// removed — build-only, covered by `build-all` (the serial talker/listener are
// also built+run by `test_qemu_serial_pubsub_e2e` below).

// =============================================================================
// Serial QEMU E2E Tests (MPS2-AN385 + CMSDK UART + zenohd serial)
// =============================================================================

/// Test serial pub/sub between two QEMU instances via zenohd serial bridge.
///
/// Architecture:
///   socat pair A:  QEMU listener UART0 ↔ zenohd serial listener
///   socat pair B:  QEMU talker  UART0 ↔ zenohd serial listener
///
/// Using socat PTY pairs ensures both ends exist before either side starts,
/// avoiding the race where firmware sends InitSyn before zenohd is ready.
/// `-display none -monitor none` avoids `-nographic`'s implicit `-serial
/// mon:stdio` which hijacks UART0 for the QEMU monitor.
#[test]
fn test_qemu_serial_pubsub_e2e() {
    require_arm_toolchain();
    require_qemu();
    require_zenoh_pico_arm();
    if !is_socat_available() {
        nros_tests::unmet!("socat not found");
    }

    // Build both binaries
    let talker_bin = build_qemu_serial_talker().require("serial-talker");
    let listener_bin = build_qemu_serial_listener().require("serial-listener");

    // Create socat PTY pairs: one for listener, one for talker.
    // Each pair links QEMU's UART0 to zenohd's serial listener.
    let tmp_dir = nros_tests::project_root().join("tmp");
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create tmp dir");

    let listener_pair = SocatPtyPair::create(
        tmp_dir.join("serial-listener-qemu").to_str().unwrap(),
        tmp_dir.join("serial-listener-zenohd").to_str().unwrap(),
    )
    .expect("Failed to create listener socat PTY pair");
    eprintln!(
        "Listener PTY pair: {} ↔ {}",
        listener_pair.qemu_path, listener_pair.zenohd_path
    );

    let talker_pair = SocatPtyPair::create(
        tmp_dir.join("serial-talker-qemu").to_str().unwrap(),
        tmp_dir.join("serial-talker-zenohd").to_str().unwrap(),
    )
    .expect("Failed to create talker socat PTY pair");
    eprintln!(
        "Talker PTY pair: {} ↔ {}",
        talker_pair.qemu_path, talker_pair.zenohd_path
    );

    // Start zenohd with serial listeners on the zenohd side of each pair.
    // zenohd must be ready before QEMU starts, so the InitSyn handshake succeeds.
    eprintln!("Starting zenohd with serial listeners...");
    let _zenohd = or_unmet(ZenohRouter::start_serial(&[
        &listener_pair.zenohd_path,
        &talker_pair.zenohd_path,
    ]));

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting serial listener QEMU...");
    let mut listener =
        QemuProcess::start_mps2_an385_with_serial(listener_bin, &listener_pair.qemu_path)
            .expect("Failed to start listener QEMU");

    // Brief delay for listener to subscribe before talker starts publishing
    std::thread::sleep(Duration::from_secs(5));

    // Start talker QEMU
    eprintln!("Starting serial talker QEMU...");
    let mut talker = QemuProcess::start_mps2_an385_with_serial(talker_bin, &talker_pair.qemu_path)
        .expect("Failed to start talker QEMU");

    // Wait for listener to receive messages (examples now run forever)
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(60),
    );

    // Wait for talker to publish messages
    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    let published = count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX);
    eprintln!(
        "Serial QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(received > 0, "Serial listener received 0 messages");
    assert!(published > 0, "Serial talker published 0 messages");
}

// =============================================================================
// Phase 207 — Bare-metal XRCE E2E (MPS2-AN385 + CMSDK UART + MicroXRCEAgent)
// =============================================================================

/// Phase 207.4 — boot the bare-metal `talker-xrce` firmware in QEMU, run
/// `MicroXRCEAgent serial` on the socat-paired PTY, assert the talker
/// reaches `Published:` (proves the XRCE session opened against the agent
/// and the publisher write succeeded — there's no `listener-xrce` shipped
/// today, so the agent-side accept is the closing edge).
///
/// Architecture:
///   socat pty pair: client0.pty  ↔  agent0.pty
///   QEMU UART0  ── client0.pty ──╫── agent0.pty ── MicroXRCEAgent serial
#[test]
fn test_qemu_xrce_pubsub_e2e() {
    require_arm_toolchain();
    require_qemu();
    if !is_socat_available() {
        nros_tests::unmet!("socat not found");
    }
    require_xrce_agent();

    let talker_bin = build_qemu_talker_xrce().require("talker-xrce");

    // Start the XRCE serial agent first (socat PTY pair + MicroXRCEAgent on
    // one end). The talker connects to the other end.
    eprintln!("Starting MicroXRCEAgent on socat PTY pair...");
    let agent = XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent");
    let client_pty = agent.client_pty_path(0).to_string();
    eprintln!("XRCE client PTY: {}", client_pty);

    // Brief delay for the agent to open its PTY end + initialize.
    std::thread::sleep(Duration::from_secs(1));

    // Boot the talker on the client PTY.
    eprintln!("Starting talker-xrce QEMU...");
    let mut talker = QemuProcess::start_mps2_an385_with_serial(talker_bin, &client_pty)
        .expect("Failed to start talker-xrce QEMU");

    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(60),
    );

    talker.kill();
    drop(agent);

    eprintln!("Talker output:\n{}", talker_output);

    let published = count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX);
    eprintln!("Bare-metal XRCE QEMU: published={}", published);

    assert!(
        published > 0,
        "talker-xrce never published — the XRCE session likely did not open against the agent"
    );
}

// =============================================================================
// RTIC QEMU Networked Tests (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================
//
// (Phase 182.3) The `test_qemu_rtic_*_builds` presence tests were removed —
// build-only, covered by `build-all` + the e2e tests below, which build+run
// the same talker/listener/server/client/mixed binaries.

#[test]
fn test_qemu_rtic_pubsub_e2e() {
    require_arm_toolchain();
    require_zenoh_pico_arm();

    // Build both binaries
    let talker_bin = build_qemu_rtic_talker().require("rtic-talker");
    let listener_bin = build_qemu_rtic_listener().require("rtic-listener");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        platform::BAREMETAL
            .zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust),
    ));

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(
            platform::BAREMETAL
                .zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust),
            Duration::from_secs(5)
        ),
        "zenohd not reachable on platform port"
    );

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting RTIC listener QEMU...");
    let mut listener = QemuProcess::start_mps2_an385_networked(listener_bin)
        .expect("Failed to start listener QEMU");

    // phase-342 W8b — wait for the listener to SAY it is subscribed, instead of
    // sleeping 8 s for "bare-metal boot + smoltcp init + zenoh connect". The
    // image prints `Waiting for messages on /chatter...` once the subscription
    // is live, which is the event this sleep was approximating; the marker comes
    // from the shared table (issue 0481 — never a literal).
    listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_WAITING_BANNER,
            Duration::from_secs(40),
        )
        .unwrap_or_else(|e| panic!("RTIC listener never subscribed: {e}"));

    // Start talker QEMU
    eprintln!("Starting RTIC talker QEMU...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin).expect("Failed to start talker QEMU");

    // Wait for listener to receive messages
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(60),
    );

    // Wait for talker to publish messages
    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    let published = count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX);
    eprintln!(
        "RTIC QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(received > 0, "RTIC QEMU listener received 0 messages");
    assert!(published > 0, "RTIC QEMU talker published 0 messages");
}

// =============================================================================
// RTIC QEMU Service/Action Networked Tests (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================

/// Service E2E test for RTIC on QEMU.
///
/// Tests 4 service calls (AddTwoInts) between server and client QEMU instances.
#[test]
fn test_qemu_rtic_service_e2e() {
    require_arm_toolchain();
    require_zenoh_pico_arm();

    // Build both binaries
    let server_bin = build_qemu_rtic_service_server().require("rtic-service-server");
    let client_bin = build_qemu_rtic_service_client().require("rtic-service-client");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        platform::BAREMETAL
            .zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust),
    ));

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(
            platform::BAREMETAL
                .zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust),
            Duration::from_secs(5)
        ),
        "zenohd not reachable on platform port"
    );

    // Start server QEMU first
    eprintln!("Starting RTIC service server QEMU...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin).expect("Failed to start server QEMU");

    // Stabilization delay: bare-metal boot + smoltcp init + zenoh connect + queryable discovery
    // Services need longer than pub/sub because zenoh queryable discovery takes time
    std::thread::sleep(Duration::from_secs(8));

    // Start client QEMU
    eprintln!("Starting RTIC service client QEMU...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin).expect("Failed to start client QEMU");

    // Wait for client to complete (it exits after 4 service calls).
    // Phase 289 (#178) — the phase-244.D1 declarative client issues ONE
    // request and logs the canonical `Result of add_two_ints:` line (the
    // shared `SERVICE_RESULT_PREFIX` constant); the old imperative client's
    // "All service calls completed" 4-call banner is retired. Grep the
    // constant, not a literal (#157 class).
    let client_output = client.collect_until(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(90),
    );

    // Collect server output
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Verify service communication
    assert!(
        client_output.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "RTIC QEMU service client never logged a service result"
    );

    let handled = count_pattern(
        &server_output,
        nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
    );
    eprintln!("RTIC QEMU service: server handled {} requests", handled);
    assert!(
        handled >= 1,
        "RTIC QEMU service server did not handle any requests (got {})",
        handled
    );
}

/// Action E2E test for RTIC on QEMU.
///
/// Tests Fibonacci action (goal, feedback, result) between server and client QEMU instances.
#[test]
fn test_qemu_rtic_action_e2e() {
    require_arm_toolchain();
    require_zenoh_pico_arm();

    // Build both binaries
    let server_bin = build_qemu_rtic_action_server().require("rtic-action-server");
    let client_bin = build_qemu_rtic_action_client().require("rtic-action-client");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        platform::BAREMETAL
            .zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust),
    ));

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(
            platform::BAREMETAL
                .zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust),
            Duration::from_secs(5)
        ),
        "zenohd not reachable on platform port"
    );

    // Start server QEMU first
    eprintln!("Starting RTIC action server QEMU...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin).expect("Failed to start server QEMU");

    // phase-342 W8b — wait for the server to SAY it is ready, instead of sleeping
    // 8 s for "bare-metal boot + smoltcp init + zenoh connect". This site is
    // inside `test_qemu_rtic_action_e2e`, so the binary is the ACTION server and
    // the marker is `Waiting for action goals...`.
    //
    // Getting this wrong is instructive: a first pass keyed on the variable name
    // (`server`) and used the SERVICE marker, which the action image never
    // prints — and the converted wait FAILED LOUDLY with "did not print
    // `Waiting for service requests` within 40s" instead of sleeping 8 s and
    // carrying on. The 8 s sleep could not have told anyone the role was wrong.
    server
        .wait_for_output_pattern(
            nros_tests::output::ACTION_SERVER_READY_MARKER,
            Duration::from_secs(40),
        )
        .unwrap_or_else(|e| panic!("RTIC action server never became ready: {e}"));

    // Start client QEMU
    eprintln!("Starting RTIC action client QEMU...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin).expect("Failed to start client QEMU");

    // Wait for the client's terminal `Result received: [...]` line; on
    // timeout the collected output is still returned, so this never captures
    // less than a blind 60 s wait.
    let client_output = client.collect_until(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(60),
    );

    // Collect server output
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Verify action communication
    assert!(
        nros_tests::output::action_goal_accepted(&client_output),
        "RTIC QEMU action client: goal was not accepted"
    );
    assert!(
        client_output.contains(nros_tests::output::ACTION_FEEDBACK_PREFIX),
        "RTIC QEMU action client did not receive feedback messages"
    );
    assert!(
        server_output.contains("Received goal request")
            || server_output.contains(nros_tests::output::ACTION_EXECUTING_MARKER),
        "RTIC QEMU action server did not accept goal"
    );
}

// =============================================================================
// RTIC Mixed-Priority QEMU Networked Test (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================

/// Mixed-priority pubsub E2E test for RTIC on QEMU.
///
/// Same as `test_qemu_rtic_pubsub_e2e` but with `publish`/`listen` at priority 2
/// and `net_poll` at priority 1. The `ffi-sync` feature prevents FFI state
/// corruption when the higher-priority task preempts `spin_once(0)`.
#[test]
fn test_qemu_rtic_mixed_priority_pubsub_e2e() {
    require_arm_toolchain();
    require_zenoh_pico_arm();

    // Build both binaries
    let talker_bin = build_qemu_rtic_mixed_talker().require("rtic-mixed-talker");
    let listener_bin = build_qemu_rtic_mixed_listener().require("rtic-mixed-listener");

    // Start zenohd (firmware connects via slirp gateway to host). The mixed
    // pair bakes its own allocator aux slot (phase-295 W4) — no sharing with
    // the plain RTIC / BSP / large-msg lanes.
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        nros_tests::alloc::BAREMETAL_MIXED_PRIORITY_PORT,
    ));

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(
            nros_tests::alloc::BAREMETAL_MIXED_PRIORITY_PORT,
            Duration::from_secs(5)
        ),
        "zenohd not reachable on platform port"
    );

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting RTIC mixed-priority listener QEMU...");
    let mut listener = QemuProcess::start_mps2_an385_networked(listener_bin)
        .expect("Failed to start listener QEMU");

    // phase-342 W8b — wait for the listener to SAY it is subscribed, instead of
    // sleeping 8 s for "bare-metal boot + smoltcp init + zenoh connect". The
    // image prints `Waiting for messages on /chatter...` once the subscription
    // is live, which is the event this sleep was approximating; the marker comes
    // from the shared table (issue 0481 — never a literal).
    listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_WAITING_BANNER,
            Duration::from_secs(40),
        )
        .unwrap_or_else(|e| panic!("RTIC listener never subscribed: {e}"));

    // Start talker QEMU
    eprintln!("Starting RTIC mixed-priority talker QEMU...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin).expect("Failed to start talker QEMU");

    // Wait for listener to receive messages
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(60),
    );

    // Wait for talker to publish messages
    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    let published = count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX);
    eprintln!(
        "RTIC mixed-priority QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(
        received > 0,
        "RTIC mixed-priority QEMU listener received 0 messages"
    );
    assert!(
        published > 0,
        "RTIC mixed-priority QEMU talker published 0 messages"
    );
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free: a cell
/// added, retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::Emulator,
        &[
            (P::QemuBaremetal, L::C, R::Zenoh, W::Action, K::Example),
            (P::QemuBaremetal, L::C, R::Zenoh, W::Pubsub, K::Example),
            (P::QemuBaremetal, L::C, R::Zenoh, W::Service, K::Example),
            (P::QemuBaremetal, L::Cpp, R::Zenoh, W::Action, K::Example),
            (P::QemuBaremetal, L::Cpp, R::Zenoh, W::Pubsub, K::Example),
            (P::QemuBaremetal, L::Cpp, R::Zenoh, W::Service, K::Example),
            (P::QemuBaremetal, L::Rust, R::Zenoh, W::Pubsub, K::Example),
        ],
    );
}

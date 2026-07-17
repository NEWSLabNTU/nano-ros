//! phase-263 B4 (QoS, C++ projection) — cross-process per-entity QoS-override round-trip in the
//! `ws-qos-cpp` workspace. The C++ sibling of the C ws-qos demo.
//!
//! QoS is a per-entity contract set IN CODE here (not via a launch `qos_overrides` section): the
//! C++ talker (`qos_talker_pkg`) publishes `std_msgs/Int32` on `/chatter` with a NON-DEFAULT QoS
//! profile — reliability=RELIABLE, durability=TRANSIENT_LOCAL, history=KEEP_LAST(10), depth=10 —
//! built via the fluent `nros::QoS` builder (`.reliable().transient_local().keep_last(10)`) and
//! passed to `Node::create_publisher`. The C++ listener (`qos_listener_pkg`) subscribes with the
//! BYTE-IDENTICAL profile and prints `Received: N`. Matching the profile is the per-entity QoS
//! contract; the two endpoints connect only because both declare the same QoS.
//!
//! The two run as TWO processes — one single-node entry each (`native_{talker,listener}_entry`,
//! booting `{talker,listener}.launch.xml`) — so asserting on the listener's stdout proves the FULL
//! cross-process delivery over the wire with the non-default QoS profile in force.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_qos_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_qos_listener_entry,
    build_native_workspace_cpp_qos_talker_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const QOS_PORT: u16 = 17931;

#[test]
fn cpp_qos_matched_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker = build_native_workspace_cpp_qos_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ qos talker entry not built: {e}"));
    let listener = build_native_workspace_cpp_qos_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ qos listener entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", QOS_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {QOS_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{QOS_PORT}");
    let _ = router;

    // Talker first (the QoS-tagged publisher boots + keeps publishing at 1 Hz), so the listener
    // joins LATE — proving the QoS-matched endpoints discover + connect across processes.
    let mut tlk = {
        let mut cmd = Command::new(&talker);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "cpp-qos-talker")
            .unwrap_or_else(|e| panic!("spawn talker: {e}"))
    };
    tlk.wait_for_output_pattern(
        nros_tests::output::INT32_TALKER_LOG_PREFIX,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        tlk.kill();
        panic!("qos_talker never published")
    });

    let mut lis = {
        let mut cmd = Command::new(&listener);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "cpp-qos-listener")
            .unwrap_or_else(|e| panic!("spawn listener: {e}"))
    };

    let out = lis
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(60),
        )
        .unwrap_or_else(|_| {
            lis.kill();
            tlk.kill();
            panic!(
                "qos_listener never received 3 QoS-matched samples — the cross-process C++ \
                 per-entity QoS-matched delivery did not work (QoS mismatch or wiring break)"
            )
        });

    lis.kill();
    tlk.kill();

    // The talker ramps the int32 0,1,2,…; the listener decodes + prints each `Received: N`. Early
    // pre-discovery samples may be missed, so assert the field appears ≥3× (proves the non-default
    // QoS profile, declared per-entity on both endpoints, connects + delivers end-to-end).
    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 QoS-matched receives, got {n}.\n{out}");
}

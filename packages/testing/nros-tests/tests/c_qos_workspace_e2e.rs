//! phase-263 B4 (QoS, C projection) — cross-process per-entity QoS-override round-trip in the
//! `ws-qos-c` workspace. The C sibling of the Rust ws-qos demo.
//!
//! QoS is a per-entity contract set IN CODE here (not via a launch `qos_overrides` section): the
//! C talker (`qos_talker_pkg`) publishes `std_msgs/Int32` on `/chatter` with a NON-DEFAULT QoS
//! profile — reliability=RELIABLE, durability=TRANSIENT_LOCAL, history=KEEP_LAST(10), depth=10 —
//! built as a `nros_cpp_qos_t` and passed by value to `nros_cpp_publisher_create` (instead of the
//! `nros_c_qos_default()` the committed pure-C talker uses). The C listener (`qos_listener_pkg`)
//! subscribes with the BYTE-IDENTICAL profile and prints `Received: N`. Matching the profile is the
//! per-entity QoS contract; the two endpoints connect only because both declare the same QoS.
//!
//! The two run as TWO processes — one single-node entry each (`native_{talker,listener}_entry`,
//! booting `{talker,listener}.launch.xml`) — so asserting on the listener's stdout proves the FULL
//! cross-process delivery over the wire with the non-default QoS profile in force.
//!
//! What this exercises: QoS-MATCHED reliable delivery with a non-default profile across processes
//! (the talker boots first and keeps publishing at 1 Hz; the listener joins late and receives the
//! live QoS-matched stream). It does NOT assert transient_local history *replay* to a late joiner
//! (zenoh-pico/zenohd do not provide durable late-join replay out of the box) — it asserts that the
//! non-default reliable+transient_local+keep_last(10) contract, declared per-entity in code on both
//! endpoints, connects and delivers. A QoS-mismatch or wiring break makes the listener receive
//! nothing → the test fails loud.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_qos_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_qos_listener_entry,
    build_native_workspace_c_qos_talker_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const QOS_PORT: u16 = 17921;

#[test]
fn c_qos_matched_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker = build_native_workspace_c_qos_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C qos talker entry not built: {e}"));
    let listener = build_native_workspace_c_qos_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C qos listener entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", QOS_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {QOS_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{QOS_PORT}");
    let _ = router;

    // Talker first (the QoS-tagged publisher boots + keeps publishing at 1 Hz), so the listener
    // joins LATE — proving the QoS-matched endpoints discover + connect across processes.
    let mut tlk = {
        let mut cmd = Command::new(&talker);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "c-qos-talker")
            .unwrap_or_else(|e| panic!("spawn talker: {e}"))
    };
    tlk.wait_for_output_pattern("Published:", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            tlk.kill();
            panic!("qos_talker never published")
        });

    let mut lis = {
        let mut cmd = Command::new(&listener);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "c-qos-listener")
            .unwrap_or_else(|e| panic!("spawn listener: {e}"))
    };

    let out = lis
        .wait_for_output_count("Received:", 3, Duration::from_secs(60))
        .unwrap_or_else(|_| {
            lis.kill();
            tlk.kill();
            panic!(
                "qos_listener never received 3 QoS-matched samples — the cross-process C \
                 per-entity QoS-matched delivery did not work (QoS mismatch or wiring break)"
            )
        });

    lis.kill();
    tlk.kill();

    // The talker ramps the int32 0,1,2,…; the listener decodes + prints each `Received: N`. Early
    // pre-discovery samples may be missed, so assert the field appears ≥3× (proves the non-default
    // QoS profile, declared per-entity on both endpoints, connects + delivers end-to-end).
    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 QoS-matched receives, got {n}.\n{out}");
}

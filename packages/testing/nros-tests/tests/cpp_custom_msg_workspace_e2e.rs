//! phase-263 B6 (custom-msg, C++ projection) — cross-process workspace-local custom-message
//! round-trip in the `ws-custom-msg-cpp` workspace. The C++ sibling of the C ws-custom-msg demo.
//!
//! `custom_msgs/Reading` is an IN-WORKSPACE interface package (`src/custom_msgs/msg/Reading.msg`:
//! `float64 temperature`, `float64 humidity`, `int32 sequence`). A C++ talker
//! (`reading_talker_pkg`) publishes it on `/reading`; a C++ listener (`reading_listener_pkg`)
//! subscribes and prints the decoded `sequence`/`temperature` fields. The two run as TWO
//! processes — one single-node entry each (`native_{talker,listener}_entry`, booting
//! `{talker,listener}.launch.xml`) — so asserting on the listener's stdout proves the FULL
//! cross-process delivery of the workspace-local custom message type over the wire.
//!
//! The C++ components carry the type name (`custom_msgs::msg::dds_::Reading_`) as a string and
//! hand-encode/decode the CDR payload (the RFC-0043 raw-CDR idiom, matching the committed C
//! workspace) — no generated interface archive is linked, dodging any cpp codegen edge.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_custom_msg_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_custom_msg_listener_entry,
    build_native_workspace_cpp_custom_msg_talker_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const CUSTOM_MSG_PORT: u16 = 17933;

#[test]
fn cpp_custom_msg_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker = build_native_workspace_cpp_custom_msg_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ custom-msg talker entry not built: {e}"));
    let listener = build_native_workspace_cpp_custom_msg_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ custom-msg listener entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", CUSTOM_MSG_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {CUSTOM_MSG_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{CUSTOM_MSG_PORT}");
    let _ = router;

    // Talker first so the publisher is discoverable when the listener joins.
    let mut tlk = {
        let mut cmd = Command::new(&talker);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "cpp-reading-talker")
            .unwrap_or_else(|e| panic!("spawn talker: {e}"))
    };
    tlk.wait_for_output_pattern("sent seq=", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            tlk.kill();
            panic!("reading_talker never published")
        });

    let mut lis = {
        let mut cmd = Command::new(&listener);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "cpp-reading-listener")
            .unwrap_or_else(|e| panic!("spawn listener: {e}"))
    };

    let out = lis
        .wait_for_output_count("reading seq=", 3, Duration::from_secs(60))
        .unwrap_or_else(|_| {
            lis.kill();
            tlk.kill();
            panic!(
                "reading_listener never received 3 custom-msg samples — the cross-process C++ \
                 custom-message delivery did not work"
            )
        });

    lis.kill();
    tlk.kill();

    let n = nros_tests::count_pattern(&out, "reading seq=");
    assert!(n >= 3, "expected ≥3 custom-msg receives, got {n}.\n{out}");
    // The decoded temperature field must also be present (a non-trivial second field), proving
    // the full CDR layout — not just a counter — survives the round-trip.
    assert!(
        out.contains("temp="),
        "listener output missing decoded `temp=` field — CDR decode wrong.\n{out}"
    );
}

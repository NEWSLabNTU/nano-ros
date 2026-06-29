//! phase-263 B6 (custom-msg, MIXED projection) — cross-process workspace-local custom-message
//! round-trip in the `ws-custom-msg-mixed` workspace. The mixed-flavored sibling of the C
//! ws-custom-msg demo: the node pkgs are the C `reading_talker_pkg` / `reading_listener_pkg`
//! reused verbatim, driven by a C++ TYPED entry carrier.
//!
//! `custom_msgs/Reading` is an IN-WORKSPACE interface package (`src/custom_msgs/msg/Reading.msg`:
//! `float64 temperature`, `float64 humidity`, `int32 sequence`). The C talker publishes it on
//! `/reading`; the C listener subscribes and prints the decoded `sequence`/`temperature` fields.
//! The two run as TWO processes — one single-node entry each (`native_{talker,listener}_entry`,
//! booting `{talker,listener}.launch.xml`) — so asserting on the listener's stdout proves the FULL
//! cross-process delivery of the workspace-local custom message type over the wire, with a C++
//! entry carrier driving the C components.
//!
//! Run with: `cargo nextest run -p nros-tests --test mixed_custom_msg_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_mixed_custom_msg_listener_entry,
    build_native_workspace_mixed_custom_msg_talker_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const CUSTOM_MSG_PORT: u16 = 17934;

#[test]
fn mixed_custom_msg_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker = build_native_workspace_mixed_custom_msg_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("MIXED custom-msg talker entry not built: {e}"));
    let listener = build_native_workspace_mixed_custom_msg_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("MIXED custom-msg listener entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", CUSTOM_MSG_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {CUSTOM_MSG_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{CUSTOM_MSG_PORT}");
    let _ = router;

    // Talker first so the publisher is discoverable when the listener joins.
    let mut tlk = {
        let mut cmd = Command::new(&talker);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "mixed-reading-talker")
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
        ManagedProcess::spawn_command(cmd, "mixed-reading-listener")
            .unwrap_or_else(|e| panic!("spawn listener: {e}"))
    };

    let out = lis
        .wait_for_output_count("reading seq=", 3, Duration::from_secs(60))
        .unwrap_or_else(|_| {
            lis.kill();
            tlk.kill();
            panic!(
                "reading_listener never received 3 custom-msg samples — the cross-process MIXED \
                 custom-message delivery did not work"
            )
        });

    lis.kill();
    tlk.kill();

    let n = nros_tests::count_pattern(&out, "reading seq=");
    assert!(n >= 3, "expected ≥3 custom-msg receives, got {n}.\n{out}");
    assert!(
        out.contains("temp="),
        "listener output missing decoded `temp=` field — CDR decode wrong.\n{out}"
    );
}

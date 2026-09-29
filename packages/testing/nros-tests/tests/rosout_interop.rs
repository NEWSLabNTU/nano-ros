//! `/rosout` — does a nano-ros node's LOG reach a stock ROS 2 operator tool?
//!
//! phase-467 Q4, ledger row `c:logging_rosout_enabled` (log.json). Interop
//! cell `native-logging-rust-zenoh-n2r`.
//!
//! # Why this needs a live peer and cannot be a unit test
//!
//! Everything below the wire is already unit-tested: the queue round-trips a
//! record (`nros_log::rosout`'s five tests), the message encodes inside its
//! derived buffer and carries rcutils's level numbering
//! (`nros_node::rosout`'s five). None of that answers the question the row is
//! about, which is whether `ros2 topic echo /rosout` — the thing an operator
//! actually types — prints anything. That question has four ways to fail that
//! a self-consistent pair of our own tests cannot see: the topic name, the
//! type name and hash, the QoS match, and whether `Log`'s CDR layout is the
//! one `rcl_interfaces` expects.
//!
//! The QoS one is the reason this cell is worth its build. We publish
//! `/rosout` **VOLATILE**, not upstream's TRANSIENT_LOCAL, because on zenoh a
//! transient-local publisher costs a `MAX_TL_PUBLISHERS` slot AND a cache
//! queryable out of an embedded budget of 8 that `[param_services]` and
//! `[lifecycle]` already claim eleven of (issues 0460 / 1378). Whether a
//! stock subscriber still matches is an RxO question about somebody else's
//! defaults, and reading rmw's tables is exactly the kind of reasoning this
//! campaign has been wrong about before.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use nros_tests::fixtures::RequireFixture;
use nros_tests::{
    fixtures, interop, output,
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, Ros2Process, require_ros2},
    skip,
};

/// The coordinate this binary covers. ONE row, and `interop::CELLS` must
/// agree — `assert_test_bound` compares the two sets.
const ROSOUT_COORDS: [(
    nros_tests::matrix::PlatformId,
    nros_tests::matrix::Lang,
    nros_tests::matrix::Rmw,
    nros_tests::matrix::Workload,
); 1] = [(
    nros_tests::matrix::PlatformId::Linux,
    nros_tests::matrix::Lang::Rust,
    nros_tests::matrix::Rmw::Zenoh,
    nros_tests::matrix::Workload::Logging,
)];

const LOG_MSG: &str = "rcl_interfaces/msg/Log";
/// `nros_node::rosout::TOPIC`, spelled out. `nros-tests` reaches `nros` with
/// `default-features = false` and no `rmw-cffi`, and the re-export is gated on
/// it, so naming the constant would mean turning a transport on in every test
/// build to read one string.
const ROSOUT_TOPIC: &str = "/rosout";
/// The peer's hard horizon. Its own `timeout --foreground`, so a wait longer
/// than this reads a truncated transcript as "no delivery" (issue 1026).
const ECHO_WINDOW: Duration = Duration::from_secs(40);
/// Shorter than the window, and a CONDITION — it returns as soon as enough
/// records land.
const ECHO_WAIT: Duration = Duration::from_secs(32);
/// How many of the probe's own records must arrive. More than one, because a
/// single record can arrive from a retained sample and says nothing about the
/// pump running per spin; and at least FOUR, because the probe raises its WARN
/// on odd iterations and the severity assertion below has to be inside the
/// window this count opens.
const WANT_RECORDS: usize = 4;

fn spawn_probe(bin: &Path, locator: &str, records: usize) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("ROSOUT_PROBE_RECORDS", records.to_string())
        .env("ROSOUT_PROBE_PERIOD_MS", "250");
    ManagedProcess::spawn_command(cmd, "rosout-talker")
        .unwrap_or_else(|e| panic!("spawn rosout-talker: {e}"))
}

/// The row's whole claim: a log call inside a nano-ros node comes out of a
/// stock `ros2 topic echo /rosout`.
///
/// NEGATIVE CONTROL, measured 2026-09-29 before the bridge existed: with no
/// `/rosout` publisher the same echo peer prints nothing at all — which is
/// the state the ledger row described, and what this test fails back to if
/// the sink is not installed or the pump is never called.
#[test]
fn a_nano_ros_log_call_reaches_ros2_topic_echo_rosout() {
    interop::assert_test_bound("rosout_interop", &ROSOUT_COORDS);

    if !require_ros2() {
        skip!("ROS 2 + rmw_zenoh_cpp not available");
    }
    let probe_bin = fixtures::build_rosout_talker().require("prebuilt rosout-talker");
    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let mut echo = match Ros2Process::topic_echo_for(
        ROSOUT_TOPIC,
        LOG_MSG,
        &locator,
        DEFAULT_ROS_DISTRO,
        ECHO_WINDOW,
    ) {
        Ok(p) => p,
        Err(e) => skip!("ROS 2 topic echo could not start: {e}"),
    };

    let mut probe = spawn_probe(probe_bin, &locator, 80);
    probe
        .wait_for_output_pattern(output::ROSOUT_PROBE_READY, Duration::from_secs(20))
        .expect("rosout-talker never installed its sink and publisher");

    // The probe's own records carry this body. Waiting on the PROBE's text
    // rather than on a bare `msg:` matters: `ros2 topic echo`'s own node also
    // publishes to `/rosout`, so `msg:` would count the peer talking to
    // itself and pass with nothing of ours on the wire.
    let (out, why) = match echo.wait_for_output_count("nros rosout probe record", WANT_RECORDS, ECHO_WAIT)
    {
        Ok(o) => (o, String::new()),
        // Diagnostic on a DIFFERENT channel from the asserted text (issue
        // 0670) — folding it in would make the counter match the complaint.
        Err(e) => (String::new(), format!("\n[wait] {e}")),
    };
    // Drain the probe's own transcript before asserting: its accounting line
    // is what tells a reader whether a short echo was a delivery failure or a
    // full queue. `wait_for_all_output` kills the process at the deadline.
    let probe_out = probe
        .wait_for_all_output(Duration::from_secs(20))
        .unwrap_or_else(|e| format!("<probe output unavailable: {e}>"));

    let seen = nros_tests::count_pattern(&out, "nros rosout probe record");
    assert!(
        seen >= WANT_RECORDS,
        "a nano-ros node's log did not reach `ros2 topic echo {}`: the stock \
         subscriber saw {seen} of the probe's records (wanted {WANT_RECORDS}).\n\
         This is the ledger row `c:logging_rosout_enabled` failing back to the \
         state it described.\n\
         Pairing: {}\n\
         nano side:\n{probe_out}\n\
         ROS 2 side:\n{out}{why}",
        ROSOUT_TOPIC,
        nros_tests::process::zenoh_pairing_versions()
    );

    // The record is not just PRESENT, it is the right SHAPE. Each of these
    // has its own way of being silently wrong on the wire.
    assert!(
        out.contains("name: rosout_probe"),
        "the logger NAME did not survive: `rqt_console` filters on it and a \
         blank one attributes every line to nothing.\nROS 2 side:\n{out}"
    );
    assert!(
        out.contains("level: 20"),
        "no INFO record (`level: 20`) arrived. `Log.level` is on rcutils's \
         number line, not `nros_log::Severity`'s 0..=5 discriminant — a `2` \
         here is our compact value leaking onto the wire.\nROS 2 side:\n{out}"
    );
    assert!(
        out.contains("level: 30"),
        "no WARN record (`level: 30`) arrived, so severity is not being \
         carried per record.\nROS 2 side:\n{out}"
    );
    assert!(
        !out.contains("sec: 0\n  nanosec: 0"),
        "every stamp is zero. `Record::timestamp_ns` is a constant 0 without \
         `nros-log/platform-clock`, and a `/rosout` stream with no time sorts \
         arbitrarily in every tool that reads it.\nROS 2 side:\n{out}"
    );

    // The probe's own accounting. A run that lost records to a full queue
    // still delivers, so the assertions above would pass — and the operator
    // would be reading an incomplete log without being told.
    assert!(
        probe_out.contains(&format!("{} pumped=", output::ROSOUT_PROBE_DONE))
            || probe_out.contains(output::ROSOUT_PROBE_PUMPED),
        "the probe never reported what it pumped:\n{probe_out}"
    );
    assert!(
        !probe_out.contains(output::ROSOUT_PROBE_PUBLISH_ERR),
        "the transport refused a /rosout publish:\n{probe_out}"
    );
    assert!(
        !probe_out.contains(output::ROSOUT_PROBE_SINK_FULL),
        "`nros_log`'s appendable sink list was full, so the bridge never \
         installed:\n{probe_out}"
    );
}

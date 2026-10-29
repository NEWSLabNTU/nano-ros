//! phase-433 W6 acceptance — make a QoS STATUS EVENT fire, and report it.
//!
//! Four vtable slots — `publisher_event_init`, `publisher_take_event`,
//! `subscription_event_init`, `subscription_take_event` — are classified
//! `produced` by `just check rmw-slot-producers` and, until this file, had
//! never been exercised against a live peer.
//!
//! Why no self-contained test could have covered them, stated as concretely as
//! `graph_interop.rs` states its own version: **a QoS event's input is another
//! process's state.** Our `rmw_event_type_t` defines five kinds, and in the
//! zenoh shim four of them are our own clock compared against our own
//! timestamps — `check_deadline_and_fire` reads `last_publish_at_ms`,
//! `check_liveliness_lost` reads `last_assert_at_ms`, and `MessageLost` counts
//! gaps in a sequence we have already received. Every one of those fires in a
//! single image with no peer at all, so running them against one proves
//! nothing that a unit test does not.
//!
//! `LivelinessChanged` on the SUBSCRIPTION side is the exception, and it is an
//! interop claim twice over:
//!
//! 1. Its trigger is the set of *remote* publishers holding an `@ros2_lv`
//!    liveliness token that matches this subscription's topic. Nothing we do
//!    to our own entity changes it.
//! 2. The token is written by `rmw_zenoh_cpp` and matched by our wildcard
//!    (`Ros2Liveliness::publisher_keyexpr_wildcard`). The tree's only existing
//!    check of that wildcard — `shim/mod.rs`, ~~`wildcard_matches_any_entity_id`~~
//!    and friends — matches it against a keyexpr **our own builder produced**.
//!    That is precisely the shape of check that was green throughout issue
//!    0903, where twelve `produced`, mutation-tested, parity-clean graph slots
//!    did not work at all.
//!
//! ## What we could NOT test, and why
//!
//! The obvious QoS event to provoke from a stock peer is incompatible QoS — a
//! RELIABLE requester against a BEST_EFFORT offerer. **Our ABI has no such
//! event kind.** `rmw_event_type_t` is `LIVELINESS_CHANGED`,
//! `REQUESTED_DEADLINE_MISSED`, MESSAGE_LOST, `LIVELINESS_LOST`,
//! `OFFERED_DEADLINE_MISSED` — upstream's `RMW_EVENT_OFFERED_QOS_INCOMPATIBLE`
//! and `RMW_EVENT_REQUESTED_QOS_INCOMPATIBLE` have no counterpart, so there is
//! nothing to register a callback for and nothing to assert.
//!
//! ## Cyclone is absent on purpose
//!
//! `native-qos-event-rust-cyclone-r2n-CARVED` records it: cyclonedds leaves
//! both `*_event_init` slots NULL, and the `*_take_event` pair it *does*
//! implement — reading real `dds_get_liveliness_changed_status` counters — has
//! no caller in the tree outside cyclonedds' own `tests/status_events.cpp`.
//! The Rust adapter hardcodes both slots to `None` and `nros-node` exposes no
//! poll API, so a Cyclone application cannot observe a status event through
//! either half of the surface. A live peer changes nothing about that; the
//! missing piece is a runtime poll path. Issue 1164.
//!
//! Interop cells (`interop::CELLS`): `native-qos-event-rust-zenoh-r2n`
//! (the Rust probe, below) and `native-qos-event-c-zenoh-bidir` (issue 2039,
//! the C API's matched + `LivelinessChanged` callbacks; see
//! [`c_matched_and_liveliness_callbacks_follow_stock_ros2_peers`]).

use nros_tests::fixtures::RequireFixture;
use std::{
    process::Command,
    time::{Duration, Instant},
};

use nros_tests::{
    fixtures, interop,
    output::{
        MATCHED_PROBE_LIVELINESS, MATCHED_PROBE_PUB, MATCHED_PROBE_READY, MATCHED_PROBE_SUB,
        MATCHED_PROBE_UNSUPPORTED, QOS_EVENT_LIVELINESS_CHANGED, QOS_EVENT_PROBE_NONE,
        QOS_EVENT_PROBE_READY, QOS_EVENT_PROBE_UNSUPPORTED, QOS_EVENT_SUPPORTS,
    },
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, Ros2Process, require_ros2},
};

/// Both cells this binary hosts. Each case states the full set because
/// `assert_test_bound` compares against every `Tier::Runtime` cell of the
/// binary; `interop::CASE_CELLS` says which case is evidence for which.
fn bound_cells() {
    interop::assert_test_bound(
        "qos_event_interop",
        &[
            (
                nros_tests::matrix::PlatformId::Linux,
                nros_tests::matrix::Lang::Rust,
                nros_tests::matrix::Rmw::Zenoh,
                nros_tests::matrix::Workload::QosEvents,
            ),
            (
                nros_tests::matrix::PlatformId::Linux,
                nros_tests::matrix::Lang::C,
                nros_tests::matrix::Rmw::Zenoh,
                nros_tests::matrix::Workload::QosEvents,
            ),
        ],
    );
}

/// A stock ROS 2 publisher appearing on our topic delivers a
/// `LivelinessChanged` event to a nano-ros application callback.
///
/// The probe's exit code carries the verdict and the markers say WHICH verdict,
/// because the three ways this can end are three different findings:
///
/// * exit 3 / `QOS_EVENT_PROBE_UNSUPPORTED` — the backend refused the kind.
/// * exit 4 / `QOS_EVENT_PROBE_NONE` — it accepted and never fired. This is
///   the one `produced` cannot see, and the reason the assertion below is on a
///   POSITIVE `alive_count_change` rather than merely on the callback running:
///   a poll that finds nothing also reports "no change".
/// * exit 0 / `QOS_EVENT LIVELINESS_CHANGED alive_change=+N` — it worked.
#[test]
fn stock_ros2_publisher_raises_a_liveliness_event() {
    // The coordinate tripwire — a cell added without a test, or a test that
    // drifts off its cell, is a failure rather than silent non-coverage. It
    // needs no fixtures and no peer, so it runs on this host too.
    //
    // Stated INLINE, as every sibling interop test does — a named `…_CELLS`
    // table here was a second spelling of `interop::CELLS`, which RFC-0051's
    // single-matrix rule forbids and `no_local_axis_tables` refuses (issue
    // 1620). One row, not two: the Cyclone shape is a `CarveOut`, and
    // `coords_for` / `assert_test_bound` compare only `Tier::Runtime` cells,
    // so listing it would make the tripwire red. `Lang::Rust` is derived from
    // the binary this test spawns — `fixtures::build_qos_event_probe()`, whose
    // `examples/fixtures.toml` row is `lang = "rust"` (phase-433 W3: the
    // language axis was once inverted and agreed with itself).
    bound_cells();

    require_ros2();
    let router = fixtures::or_unmet(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    // The talker starts FIRST and the probe's `alive_count` starts at zero, so
    // the first poll that resolves the token is a 0 -> 1 transition. Starting
    // the probe first would be more faithful to "a peer appeared", but it makes
    // the observable depend on whether the peer came up inside the budget,
    // which is a race and not the claim.
    let _talker = Ros2Process::demo_nodes_cpp_talker(&locator, DEFAULT_ROS_DISTRO)
        .expect("start the stock talker");

    let probe = fixtures::build_qos_event_probe().require("prebuilt qos-event-probe");
    let out = Command::new(probe)
        .env("NROS_LOCATOR", &locator)
        .env("QOS_EVENT_PROBE_TOPIC", "/chatter")
        .env("QOS_EVENT_PROBE_TIMEOUT_MS", "25000")
        .output()
        .expect("run qos-event-probe");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Name the two failure modes before the generic status assertion, so the
    // message says which one happened instead of printing an exit code.
    assert!(
        !output.contains(QOS_EVENT_PROBE_UNSUPPORTED),
        "the zenoh backend must accept a `LivelinessChanged` registration — \
         `supports_event` says it does. Probe said:\n{output}"
    );
    assert!(
        !output.contains(QOS_EVENT_PROBE_NONE),
        "the callback was registered and never fired against a LIVE \
         `rmw_zenoh_cpp` talker holding an `@ros2_lv` token on this topic. \
         This is the finding phase-433 W6 exists to be able to state: the slot \
         is `produced` and inert. Probe said:\n{output}"
    );
    assert!(
        out.status.success(),
        "qos-event-probe must exit 0 once a liveliness event arrives; got {:?}\n{output}",
        out.status.code()
    );

    // The probe reached the point where a verdict is meaningful at all. Without
    // this, a probe that died before registering would satisfy the two
    // "does not contain" assertions above by printing nothing — the classic
    // absorbing-STALE shape (issue 0445).
    assert!(
        output.contains(QOS_EVENT_PROBE_READY),
        "the probe must reach READY (subscription + callback in place) before \
         any verdict here means anything:\n{output}"
    );
    assert!(
        output.contains(QOS_EVENT_SUPPORTS),
        "the probe must report the backend's declared capability:\n{output}"
    );
    assert!(
        output.contains(QOS_EVENT_LIVELINESS_CHANGED),
        "a `LivelinessChanged` event must reach the application callback:\n{output}"
    );

    // The DELTA, not just the line. `alive_change=0` is what a poll that
    // resolves no tokens reports, and it would satisfy the marker above while
    // proving the opposite of what this test claims.
    let positive = output.lines().any(|l| {
        l.contains(QOS_EVENT_LIVELINESS_CHANGED)
            && l.split_whitespace()
                .find_map(|f| f.strip_prefix("alive_change="))
                .and_then(|v| v.parse::<i32>().ok())
                .is_some_and(|d| d > 0)
    });
    assert!(
        positive,
        "the event must report a POSITIVE `alive_count_change` — the stock \
         talker appearing. A zero delta is what a liveliness poll that resolved \
         no tokens also reports:\n{output}"
    );
}

/// One field of a probe marker line (`current=3` -> 3).
fn field(line: &str, key: &str) -> Option<i64> {
    line.split_whitespace()
        .find_map(|f| f.strip_prefix(key))
        .and_then(|v| v.parse::<i64>().ok())
}

/// The `current=` value of the LAST `marker` line in `transcript`.
fn last_current(transcript: &str, marker: &str) -> Option<i64> {
    transcript
        .lines()
        .filter(|l| l.contains(marker))
        .filter_map(|l| field(l, "current="))
        .next_back()
}

/// Keep reading the probe into `transcript` until `done(transcript)` or the
/// deadline. `collect_until` hands back only what arrived during the call, so
/// the transcript is the caller's to keep.
fn pump_until(
    probe: &mut ManagedProcess,
    transcript: &mut String,
    within: Duration,
    done: impl Fn(&str) -> bool,
) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if done(transcript) {
            return true;
        }
        transcript.push_str(&probe.collect_until("\u{0}", Duration::from_millis(500)));
    }
    done(transcript)
}

/// Issue 2039 — the C API's matched and status-event callbacks, driven by
/// STOCK ROS 2 peers (cell `native-qos-event-c-zenoh-bidir`).
///
/// The probe (`bins/matched-events-probe-c`) publishes on `/i2039_out` and
/// subscribes to `/i2039_in`, with three callbacks registered through the
/// public setters. Then:
///
/// 1. a stock `ros2 topic echo /i2039_out` and a stock `ros2 topic pub
///    /i2039_in` start, and the C callbacks must report the counts RISING:
///    the publisher's matched `current >= 1`, the subscription's matched
///    `current >= 1`, and a `LivelinessChanged` with a positive
///    `alive_change` (the stock publisher appearing);
/// 2. both peers stop, and BOTH matched callbacks must report `current`
///    falling back to 0 with a negative `change=`, and the `LivelinessChanged`
///    callback a negative `alive_change` (the stock publisher leaving).
///
/// The subscription's half of (2) is issue 2060's regression check. The zenoh
/// shim observes a subscription's matched / liveliness / deadline status inside
/// `has_data`, and the executor used to skip `has_data` for a wake-attributed
/// slot with no arrival, so once the last publisher was gone nothing ever
/// asked and the count never fell. A subscription with a status callback is now
/// probed every pass (`Executor::entity_status_polled`). The publisher side
/// always fell, because `rcl_publish` is its poll point.
///
/// The path under test is nros-c's — the process-wide `status_event_slots`
/// table and the generation-tagged slot reference between the backend and
/// the user's callback — which the Rust cell above does not reach.
#[test]
fn c_matched_and_liveliness_callbacks_follow_stock_ros2_peers() {
    bound_cells();

    require_ros2();
    let router = fixtures::or_unmet(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let bin = fixtures::build_cmake_leaf_rmw(
        "packages/testing/nros-tests/bins/matched-events-probe-c",
        "matched_events_probe_c",
        nros_tests::fixtures::Rmw::Zenoh,
    )
    .require("prebuilt matched-events-probe-c");
    let mut cmd = Command::new(&bin);
    cmd.env("NROS_LOCATOR", &locator)
        .env("MATCHED_EVENTS_PROBE_TIMEOUT_MS", "90000");
    let mut probe = ManagedProcess::spawn_command(cmd, "matched-events-probe-c")
        .unwrap_or_else(|e| panic!("spawn matched-events-probe-c: {e}"));

    let mut transcript = String::new();
    let ready = pump_until(&mut probe, &mut transcript, Duration::from_secs(20), |t| {
        t.contains(MATCHED_PROBE_READY) || t.contains(MATCHED_PROBE_UNSUPPORTED)
    });
    assert!(
        !transcript.contains(MATCHED_PROBE_UNSUPPORTED),
        "the zenoh backend refused a C matched / liveliness registration:\n{transcript}"
    );
    assert!(
        ready,
        "the probe never reached READY (publisher, subscription and three \
         callbacks in place):\n{transcript}"
    );

    // Phase 1 — the stock peers appear.
    let mut echo = Ros2Process::topic_echo_for(
        "/i2039_out",
        "std_msgs/msg/String",
        &locator,
        DEFAULT_ROS_DISTRO,
        Duration::from_secs(60),
    )
    .expect("start the stock subscriber");
    let mut publisher = Ros2Process::topic_pub(
        "/i2039_in",
        "std_msgs/msg/String",
        "{data: i2039}",
        2,
        &locator,
        DEFAULT_ROS_DISTRO,
    )
    .expect("start the stock publisher");

    let risen = |t: &str| {
        last_current(t, MATCHED_PROBE_PUB).is_some_and(|c| c >= 1)
            && last_current(t, MATCHED_PROBE_SUB).is_some_and(|c| c >= 1)
            && t.lines().any(|l| {
                l.contains(MATCHED_PROBE_LIVELINESS)
                    && field(l, "alive_change=").is_some_and(|d| d > 0)
            })
    };
    let rose = pump_until(&mut probe, &mut transcript, Duration::from_secs(40), risen);
    assert!(
        rose,
        "with a stock `ros2 topic echo` and `ros2 topic pub` running, the C \
         callbacks must report: publisher matched current >= 1, subscription \
         matched current >= 1, and a LivelinessChanged with alive_change > 0.\n\
         Probe said:\n{transcript}"
    );
    // The rise is a CHANGE, not a level: a positive `change=` on each side.
    for marker in [MATCHED_PROBE_PUB, MATCHED_PROBE_SUB] {
        assert!(
            transcript
                .lines()
                .any(|l| l.contains(marker) && field(l, "change=").is_some_and(|d| d > 0)),
            "`{marker}` never reported a positive current_count_change:\n{transcript}"
        );
    }

    // Phase 2 — the stock peers leave; both counts must fall to 0, and the
    // stock publisher's liveliness must drop.
    echo.kill();
    publisher.kill();
    let fallen = |t: &str| {
        last_current(t, MATCHED_PROBE_PUB) == Some(0)
            && last_current(t, MATCHED_PROBE_SUB) == Some(0)
            && t.lines().any(|l| {
                l.contains(MATCHED_PROBE_LIVELINESS)
                    && field(l, "alive_change=").is_some_and(|d| d < 0)
            })
    };
    let fell = pump_until(&mut probe, &mut transcript, Duration::from_secs(40), fallen);
    probe.kill();
    assert!(
        fell,
        "after the stock peers stopped, the C callbacks must report: publisher \
         matched current 0, subscription matched current 0 (issue 2060), and a \
         LivelinessChanged with alive_change < 0.\nProbe said:\n{transcript}"
    );
    for marker in [MATCHED_PROBE_PUB, MATCHED_PROBE_SUB] {
        assert!(
            transcript
                .lines()
                .any(|l| l.contains(marker) && field(l, "change=").is_some_and(|d| d < 0)),
            "`{marker}` never reported a negative current_count_change:\n{transcript}"
        );
    }
}

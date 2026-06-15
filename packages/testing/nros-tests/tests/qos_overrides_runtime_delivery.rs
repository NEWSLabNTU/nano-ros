//! Phase 211.H — `qos_overrides` honoured at runtime on a LIVE entity.
//!
//! The planner's lowering of `qos_overrides.<topic>.<role>.<policy>` launch
//! params into the entry's `&'static [QosOverride]` table is covered by unit
//! tests (`plan_system_lowers_qos_overrides`, `render_sub_qos_expr_bakes_*`).
//! What those can't show is the LAST link: that the baked table actually
//! changes a running entity's QoS and the entity still delivers. This test
//! closes that gap.
//!
//! The `qos-override-pubsub` fixture installs the override on its `NodeHandle`
//! via `set_qos_overrides` (exactly what a baked entry does), creating a raw
//! publisher / subscription on `/chatter`. The override is folded into the
//! entity at create time by `create_publisher_raw_with_qos` /
//! `create_subscription_raw` (the wired runtime path). Each role logs the
//! effective profile through the SAME `QosSettings::apply_overrides` the create
//! path runs — so the logged profile IS the live entity's.
//!
//! ## Why cross-process
//!
//! Same zenoh-pico in-process "write filter" limitation as
//! `deployed_native_system_e2e`: a publisher + subscriber in one OS process
//! never exchange. So delivery is observed across two separate processes via a
//! shared `zenohd`.
//!
//! ## What is asserted
//!
//! * **Override applied + delivers** — with `reliability=best_effort`, both
//!   processes log `reliability=BestEffort` and the subscriber receives the
//!   publisher's samples cross-process.
//! * **Baseline contrast** — with NO override the publisher's entity keeps the
//!   default `reliability=Reliable`, proving the override mechanism (not a
//!   constant) is what flipped the live profile in the case above.

use std::{process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_qos_override_pubsub, require_zenohd, zenohd_unique,
    },
};
use rstest::rstest;

fn qos_cmd(
    bin: &std::path::Path,
    role: &str,
    locator: &str,
    override_spec: Option<&str>,
) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info")
        .env("NROS_QOS_ROLE", role)
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    match override_spec {
        Some(spec) => {
            cmd.env("NROS_QOS_OVERRIDE", spec);
        }
        None => {
            cmd.env_remove("NROS_QOS_OVERRIDE");
        }
    }
    cmd
}

/// A `reliability=best_effort` override from the (simulated) launch plan is
/// folded into both the live publisher and subscriber, and the subscriber
/// receives the publisher's samples cross-process.
#[rstest]
fn qos_override_best_effort_honored_and_delivers(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let bin = match build_qos_override_pubsub() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("qos-override-pubsub fixture not built: {e}"),
    };
    let locator = zenohd_unique.locator();

    // Subscriber first so its declaration precedes the publisher's samples.
    let mut listener = ManagedProcess::spawn_command(
        qos_cmd(&bin, "listener", &locator, Some("reliability=best_effort")),
        "qos-listener",
    )
    .expect("spawn listener");
    listener
        .wait_for_output_pattern(
            "qos effective: role=Subscription reliability=BestEffort",
            Duration::from_secs(8),
        )
        .expect("subscriber did not log the BestEffort override on its live entity");
    listener
        // The zenoh-pico subscription declaration between the `qos effective`
        // log and `Waiting for` can take several seconds on the loaded 2-vCPU CI
        // runner (passes in ~2 s locally). 4 s was too tight — it timed out on
        // host-integration. Match the other waits' headroom (issue #57 triage).
        .wait_for_output_pattern("Waiting for", Duration::from_secs(12))
        .expect("subscriber did not become ready");

    let mut talker = ManagedProcess::spawn_command(
        qos_cmd(&bin, "talker", &locator, Some("reliability=best_effort")),
        "qos-talker",
    )
    .expect("spawn talker");
    let talker_qos = talker
        .wait_for_output_pattern(
            "qos effective: role=Publisher reliability=BestEffort",
            Duration::from_secs(8),
        )
        .expect("publisher did not log the BestEffort override on its live entity");
    assert!(
        talker_qos.contains("reliability=BestEffort"),
        "publisher entity must carry the best_effort override:\n{talker_qos}"
    );

    let listener_output = listener
        .wait_for_output_count("Received:", 2, Duration::from_secs(15))
        .expect("subscriber received nothing under the best_effort override");

    talker.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    assert!(
        received >= 2,
        "delivery must succeed under the runtime qos override (Received = {received}):\n{listener_output}"
    );
}

/// Baseline contrast: without an override the live publisher keeps the default
/// `Reliable` profile — so the `BestEffort` in the test above is the override
/// taking effect, not a constant the fixture always prints.
#[rstest]
fn qos_default_without_override_is_reliable(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let bin = match build_qos_override_pubsub() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("qos-override-pubsub fixture not built: {e}"),
    };
    let locator = zenohd_unique.locator();

    let mut talker = ManagedProcess::spawn_command(
        qos_cmd(&bin, "talker", &locator, None),
        "qos-talker-default",
    )
    .expect("spawn talker");
    let qos = talker
        .wait_for_output_pattern("qos effective: role=Publisher", Duration::from_secs(8))
        .expect("publisher did not log its effective qos");
    talker.kill();

    assert!(
        qos.contains("reliability=Reliable"),
        "without an override the publisher entity must keep the default Reliable profile:\n{qos}"
    );
}

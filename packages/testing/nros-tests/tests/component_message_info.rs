//! A component reads the message's `MessageInfo` (issue 2349).
//!
//! `message-info-component` is a `Component` whose subscription is declared
//! with `create_subscription_for_callback_name_with_message_info`, registered
//! through the declarative runtime, and prints what `CallbackCtx::message_info()`
//! returns. Against a plain talker over zenohd, the same facts
//! `zero_copy::test_zero_copy_message_info` asserts for the executor's builder
//! must hold here: every message carries metadata, the sequence number rises,
//! and the publisher GID is stable and non-zero.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_message_info_component,
        build_native_talker, require_zenohd, zenohd_unique,
    },
    output,
};
use rstest::rstest;
use std::time::Duration;

/// The text after `key` on `line`, up to the next space.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = &line[line.find(key)? + key.len()..];
    Some(&rest[..rest.find(' ').unwrap_or(rest.len())])
}

#[rstest]
fn a_component_subscription_reads_each_messages_info(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    require_zenohd();

    let talker_path = build_native_talker().require("talker");
    let component_path = build_message_info_component().require("the MessageInfo component");
    let locator = zenohd_unique.locator();

    let mut listener_cmd = Command::new(component_path);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "message-info-component")
        .expect("start the MessageInfo component");
    listener
        .wait_for_output_pattern(output::LISTENER_READY_MARKER, Duration::from_secs(10))
        .expect("the MessageInfo component did not start");

    let mut talker_cmd = Command::new(talker_path);
    talker_cmd.env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "talker").expect("start talker");

    let out = listener
        .wait_for_output_count(output::MESSAGE_INFO_LOG_PREFIX, 3, Duration::from_secs(30))
        .expect("the component printed fewer than 3 MessageInfo lines");
    talker.kill();
    listener.kill();
    eprintln!("component output:\n{out}");

    let heard = out.matches(output::LISTENER_LOG_PREFIX).count();
    let absent = out.matches(output::MESSAGE_INFO_ABSENT).count();
    assert_eq!(
        absent, 0,
        "a message reached the component's callback with no MessageInfo:\n{out}"
    );

    let lines: Vec<&str> = out
        .lines()
        .filter(|l| l.contains(output::MESSAGE_INFO_LOG_PREFIX))
        .collect();
    assert!(
        lines.len() >= 3 && lines.len() <= heard,
        "{} MessageInfo lines for {heard} messages:\n{out}",
        lines.len()
    );

    let seqs: Vec<i64> = lines
        .iter()
        .filter_map(|l| field(l, output::MESSAGE_INFO_LOG_PREFIX)?.parse().ok())
        .collect();
    assert_eq!(seqs.len(), lines.len(), "an unparsable seq= field:\n{out}");
    for w in seqs.windows(2) {
        assert!(w[1] > w[0], "sequence numbers must rise: {seqs:?}");
    }

    let gids: Vec<&str> = lines.iter().filter_map(|l| field(l, "gid=")).collect();
    assert_eq!(gids.len(), lines.len(), "a missing gid= field:\n{out}");
    assert!(
        gids.iter().all(|g| *g == gids[0]),
        "one publisher, one GID: {gids:?}"
    );
    assert!(
        gids[0].chars().any(|c| c != '0'),
        "the GID is all zeros, so no real publisher id reached the component: {}",
        gids[0]
    );
}

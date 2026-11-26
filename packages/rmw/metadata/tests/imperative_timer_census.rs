//! Issue 2985: a timer or guard condition Rust code creates through a `Node`
//! method reaches the census.
//!
//! Timers and guard conditions never touch the RMW, so the census counts a
//! callback slot for one only when a hook reports it. The C, C++ and
//! declarative Rust routes call `census_hooks::on_timer_create` /
//! `on_guard_condition_create`. The imperative Rust routes
//! (`Node::create_guard_condition` and the `Node::create_timer_*` family) live
//! in `nros-node`, below those hooks, and recorded nothing, so a census of
//! Rust code that created one that way was a slot short: `ExecutorFull` at
//! boot on an image sized from it.
//!
//! This drives every one of those `Node` methods on an executor opened over
//! the transport-free `metadata` backend and reads the sidecar the census is
//! built from: each is one `timers[]` row, of its own kind, recorded once.

use nros::{TimerClockSource, TimerDuration};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// The `"kind"` of every row of every node's `"timers"` array, in order.
fn timer_kinds(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in json.match_indices("\"timers\":[") {
        let rest = &json[at + "\"timers\":[".len()..];
        // The array ends at the first `]` at depth 0.
        let mut depth = 0i32;
        let mut end = rest.len();
        for (i, c) in rest.char_indices() {
            match c {
                '{' | '[' => depth += 1,
                '}' => depth -= 1,
                ']' if depth == 0 => {
                    end = i;
                    break;
                }
                ']' => depth -= 1,
                _ => {}
            }
        }
        out.extend(
            rest[..end]
                .split("\"kind\":\"")
                .skip(1)
                .map(|s| s.split('"').next().unwrap_or("").to_owned()),
        );
    }
    assert!(
        !out.is_empty() || json.contains("\"timers\":["),
        "no timers array: {json}"
    );
    out
}

#[test]
fn imperative_rust_timers_and_guards_reach_the_census() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    // What `begin_node` does on a census run's first node; this test has no
    // hooked node, so it arms the observers itself.
    nros::metadata_mode::listen_for_registrations();

    // Two executors of four slots each (the default `NROS_EXECUTOR_MAX_CBS`
    // this test builds with), one node each, between them every `Node` method.
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let ms = TimerDuration::from_millis;

    let mut first = nros::Executor::open(&config).expect("the metadata backend must open");
    let id = first
        .node_builder("clocked")
        .build()
        .expect("node clocked registers");
    {
        let mut node = first.node_mut(id);
        let group = node.create_callback_group("ctrl");
        node.create_timer_repeating(ms(10), || {})
            .expect("repeating");
        node.create_timer_oneshot(ms(20), || {}).expect("oneshot");
        node.create_timer_inert(ms(30), || {}).expect("inert");
        node.create_timer_in_group(&group, ms(40), || {})
            .expect("in group");
    }

    let mut second = nros::Executor::open(&config).expect("the metadata backend must open");
    let id = second
        .node_builder("woken")
        .build()
        .expect("node woken registers");
    {
        let mut node = second.node_mut(id);
        let group = node.create_callback_group("ctrl");
        node.create_timer_on_clock(ms(50), TimerClockSource::Steady, || {})
            .expect("on clock");
        node.create_timer_on_clock_in_group(&group, ms(60), TimerClockSource::Steady, || {})
            .expect("on clock in group");
        node.create_timer_with_info(ms(70), TimerClockSource::Steady, true, |_info| {})
            .expect("with info");
        let _ = node.create_guard_condition(|| {}).expect("guard");
    }

    let export = nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "imperative")
        .executable("imperative")
        .language("rust");
    let json = nros::metadata_mode::to_json(&export).expect("serialize");

    let mut kinds = timer_kinds(&json);
    kinds.sort();
    let mut want = vec![
        "clock",
        "clock",
        "guard_condition",
        "in_group",
        "oneshot",
        "wall",
        "wall",
        "wall",
    ];
    want.sort_unstable();
    assert_eq!(
        kinds, want,
        "one row per Rust timer and guard condition, each of its own kind: {json}"
    );
}

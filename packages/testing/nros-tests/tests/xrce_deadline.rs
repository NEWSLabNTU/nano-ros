//! Issue 2108 — on XRCE, a subscription's REQUESTED_DEADLINE_MISSED is
//! counted client-side and delivered from the executor's own spin, through a
//! live Agent.
//!
//! The XRCE protocol carries no DDS status to the client, so the miss is the
//! backend's own clock against the last takeable arrival (`xrce_deadline_*` in
//! `session.c`), checked at the end of every `drive_io`. Two halves:
//!
//! 1. no publisher: a 100 ms deadline over ~550 ms of spinning reports the
//!    whole periods that ended — at least 4, at most 6, never one per spin;
//! 2. a publisher on the same topic every 20 ms, its samples taken: the count
//!    stops (at most one more, for the round trip of the first sample).
//!
//! Before issue 2108 the registration answered UNSUPPORTED (the event slots
//! were NULL), which is the first assertion here — the negative control is
//! reverting the vtable entries.
//!
//! Run: `just check required-features-tests` (needs the XRCE Agent).

#![cfg(feature = "xrce-wake-test")]

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nros_node::executor::*;
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

const TYPE: &str = "std_msgs/msg/Empty";
const PAYLOAD: &[u8] = &[0, 1, 0, 0];
const DEADLINE: Duration = Duration::from_millis(100);

#[derive(Default, Clone, Copy, Debug)]
struct Seen {
    calls: u32,
    total: u32,
    change_sum: u32,
}

fn spin_for(exec: &mut Executor<'static>, d: Duration, mut each: impl FnMut()) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        exec.spin_once(Duration::from_millis(10));
        each();
    }
}

#[test]
fn xrce_subscription_reports_requested_deadline_misses() {
    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");

    let _ = nros_rmw_xrce_cffi::register();
    let addr = agent.addr();
    let domain = u32::from(nros_tests::unique_ros_domain_id());
    let config = ExecutorConfig::new(&addr)
        .node_name("xrce_deadline")
        .domain_id(domain);
    let mut exec = Executor::open(&config).expect("Executor::open over XRCE");
    let node = exec.node_builder("xrce_deadline").build().expect("node");

    let topic = "/xrce_deadline/a";
    let mut sub = exec
        .with_node_try(node, |n| n.create_subscription_raw(topic, TYPE, ""))
        .expect("subscription");
    let seen = Arc::new(Mutex::new(Seen::default()));
    let s2 = Arc::clone(&seen);
    sub.on_requested_deadline_missed(DEADLINE, move |st| {
        let mut s = s2.lock().unwrap();
        s.calls += 1;
        s.total = st.total_count;
        s.change_sum += st.total_count_change;
    })
    .expect("register on_requested_deadline_missed over XRCE");

    // (1) No publisher.
    let start = Instant::now();
    spin_for(&mut exec, Duration::from_millis(550), || {});
    let quiet = *seen.lock().unwrap();
    let elapsed = start.elapsed();
    assert!(
        quiet.calls >= 1 && quiet.total >= 4 && quiet.total <= 6,
        "a 100 ms deadline over {elapsed:?} with no publisher reported {quiet:?}; expected 4..=6 \
         missed periods"
    );
    assert_eq!(
        quiet.change_sum, quiet.total,
        "total_count_change must sum to total_count: {quiet:?}"
    );
    assert!(
        quiet.calls < 50,
        "one callback per spin, not per missed period: {quiet:?}"
    );

    // (2) A publisher every 20 ms, samples taken: the period keeps restarting.
    let publisher = exec
        .node_mut(node)
        .create_publisher_raw(topic, TYPE, "")
        .expect("publisher");
    let mut got = 0u32;
    let mut first_at: Option<Instant> = None;
    let warm_end = Instant::now() + Duration::from_secs(10);
    // Warm up: until samples flow (discovery inside the Agent's participant).
    while first_at.is_none() && Instant::now() < warm_end {
        publisher.publish_serialized(PAYLOAD).expect("publish");
        exec.spin_once(Duration::from_millis(20));
        while let Ok(Some(_)) = sub.take_serialized() {
            got += 1;
            first_at.get_or_insert_with(Instant::now);
        }
    }
    assert!(first_at.is_some(), "no sample came back through the Agent");
    let before = *seen.lock().unwrap();
    let mut last_pub = Instant::now();
    spin_for(&mut exec, Duration::from_millis(600), || {
        if last_pub.elapsed() >= Duration::from_millis(20) {
            let _ = publisher.publish_serialized(PAYLOAD);
            last_pub = Instant::now();
        }
        while let Ok(Some(_)) = sub.take_serialized() {
            got += 1;
        }
    });
    let after = *seen.lock().unwrap();
    assert!(
        after.total <= before.total + 1,
        "samples every 20 ms against a 100 ms deadline still reported misses: before {before:?}, \
         after {after:?} ({got} samples taken)"
    );
    drop(publisher);
    drop(sub);
    let _ = exec.close();
    drop(agent);
    println!("quiet {quiet:?}; with traffic {before:?} -> {after:?}; {got} samples");
}

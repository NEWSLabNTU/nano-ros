//! Issue 2109 — on XRCE, an arrival names its entity: the per-entity
//! new-data callback (`subscription_set_on_new_message_callback`) fires for
//! the subscription the sample is for, and for no other.
//!
//! The zenoh sibling is `entity_wake.rs`. XRCE has no background thread, so an
//! arrival can only happen inside `drive_io`; the test drives the session
//! directly (not `spin_once`, whose dispatch would consume the ready bit before
//! it could be read) and reads the executor's ready word after each drive.
//!
//! Before issue 2109 the slot was NULL, so the executor armed no XRCE slot at
//! all — `entity_attributed_slots()` stayed 0 and the first assertion below
//! fails. That is the negative control: revert the three vtable entries and
//! this test goes red there.
//!
//! Run: `just check required-features-tests` (needs the XRCE Agent).

#![cfg(feature = "xrce-wake-test")]

use std::time::{Duration, Instant};

use nros_node::{QoSProfile, Session as _, executor::*};
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

const TYPE: &str = "std_msgs/msg/Empty";
/// A CDR encapsulation header and nothing else.
const PAYLOAD: &[u8] = &[0, 1, 0, 0];
/// Discovery inside the Agent's participant plus the round trip.
const ARRIVAL_BUDGET: Duration = Duration::from_secs(10);

fn subscribe(exec: &mut Executor<'static>, node: NodeId, topic: &str) -> u64 {
    let before = exec.entity_attributed_slots();
    exec.node_mut(node)
        .subscription(topic)
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>()
        .build(|_data: &[u8]| {})
        .expect("register subscription");
    let armed = exec.entity_attributed_slots() & !before;
    assert_eq!(
        armed.count_ones(),
        1,
        "subscribing to {topic} armed {armed:#x}: XRCE's per-entity callback slot must accept \
         the executor's waker (issue 2109), so exactly one new slot is attributed"
    );
    armed
}

/// Publish until a drive of the session marks something, then drain a little
/// longer and return every bit collected.
fn publish_until_marked(
    exec: &mut Executor<'static>,
    publisher: &nros_node::EmbeddedRawPublisher,
) -> u64 {
    let start = Instant::now();
    let mut bits = 0;
    while bits == 0 {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "no entity wake within {ARRIVAL_BUDGET:?} of publishing"
        );
        publisher.publish_serialized(PAYLOAD).expect("publish");
        let _ = exec.session_mut().drive_io(50);
        bits |= exec.take_entity_ready_bits();
    }
    for _ in 0..4 {
        let _ = exec.session_mut().drive_io(50);
    }
    bits | exec.take_entity_ready_bits()
}

#[test]
fn each_xrce_arrival_marks_its_own_subscription() {
    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");

    let _ = nros_rmw_xrce_cffi::register();
    let addr = agent.addr();
    let domain = u32::from(nros_tests::unique_ros_domain_id());
    let config = ExecutorConfig::new(&addr)
        .node_name("xrce_entity_wake")
        .domain_id(domain);
    let mut exec = Executor::open(&config).expect("Executor::open over XRCE");
    let node = exec.node_builder("xrce_entity_wake").build().expect("node");

    let topics = ["/xrce_entity_wake/a", "/xrce_entity_wake/b"];
    let slots: Vec<u64> = topics
        .iter()
        .map(|t| subscribe(&mut exec, node, t))
        .collect();
    // Same session: the Agent's participant matches its own writer and reader,
    // so the sample comes back through the Agent like any other.
    let publishers: Vec<_> = topics
        .iter()
        .map(|t| {
            exec.node_mut(node)
                .create_publisher_raw(t, TYPE, "")
                .expect("publisher")
        })
        .collect();

    // Nothing published yet, so nothing may be marked — otherwise a non-zero
    // word later proves nothing.
    for _ in 0..4 {
        let _ = exec.session_mut().drive_io(50);
    }
    assert_eq!(
        exec.take_entity_ready_bits(),
        0,
        "the ready word holds bits before any arrival"
    );

    for (i, publisher) in publishers.iter().enumerate() {
        let bits = publish_until_marked(&mut exec, publisher);
        assert_eq!(
            bits, slots[i],
            "publishing on {} marked {bits:#x}; its subscription is slot {:#x}",
            topics[i], slots[i]
        );
    }
    drop(publishers);
    let _ = exec.close();
    drop(agent);
}

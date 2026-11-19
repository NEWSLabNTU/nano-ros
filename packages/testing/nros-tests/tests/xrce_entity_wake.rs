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

/// Issue 2542 -- the executor-level half of the same claim: an XRCE arrival
/// is dispatched from its entity's ready bit, so the spin that runs the
/// callback asks no `has_data`. The zenoh sibling is
/// `entity_wake::a_pushed_spin_dispatches_without_probing_readiness`.
///
/// The callback count is the lower bound: a spin that skipped the arrival
/// would also make 0 probes, so the assertion is only read on the spin that
/// DID dispatch. Each such spin's probe count is printed
/// (`probes per dispatching spin: N`), which is how the before/after number
/// in issue 2542 was taken: with the three vtable slots reverted to NULL the
/// slot is unattributed and that spin probes it.
#[test]
fn a_pushed_xrce_spin_dispatches_without_probing_readiness() {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");

    let _ = nros_rmw_xrce_cffi::register();
    let addr = agent.addr();
    let domain = u32::from(nros_tests::unique_ros_domain_id());
    let config = ExecutorConfig::new(&addr)
        .node_name("xrce_entity_wake_pushed")
        .domain_id(domain);
    let mut exec = Executor::open(&config).expect("Executor::open over XRCE");
    let node = exec
        .node_builder("xrce_entity_wake_pushed")
        .build()
        .expect("node");
    let topic = "/xrce_entity_wake/pushed";
    let received = Arc::new(AtomicU32::new(0));
    let received_cb = received.clone();
    let before = exec.entity_attributed_slots();
    exec.node_mut(node)
        .subscription(topic)
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>()
        .build(move |_data: &[u8]| {
            received_cb.fetch_add(1, Ordering::SeqCst);
        })
        .expect("register subscription");
    assert_eq!(
        (exec.entity_attributed_slots() & !before).count_ones(),
        1,
        "the XRCE subscription was not attributed, so its readiness can only be probed"
    );
    let publisher = exec
        .node_mut(node)
        .create_publisher_raw(topic, TYPE, "")
        .expect("publisher");

    // The first spin after arming asks `has_data` once, for data that may
    // predate the registration. After that an idle spin asks nothing.
    exec.spin_once(Duration::from_millis(20));
    let idle_from = exec.readiness_probe_count();
    exec.spin_once(Duration::from_millis(20));
    assert_eq!(
        exec.readiness_probe_count() - idle_from,
        0,
        "an idle spin over one attributed XRCE subscription probed its readiness"
    );

    let start = Instant::now();
    let mut last_publish: Option<Instant> = None;
    loop {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "the XRCE subscription dispatched nothing within {ARRIVAL_BUDGET:?} of publishing"
        );
        if last_publish.is_none_or(|t| t.elapsed() > Duration::from_millis(500)) {
            publisher.publish_serialized(PAYLOAD).expect("publish");
            last_publish = Some(Instant::now());
        }
        let probes_before = exec.readiness_probe_count();
        exec.spin_once(Duration::from_millis(100));
        let probes = exec.readiness_probe_count() - probes_before;
        if received.load(Ordering::SeqCst) > 0 {
            eprintln!("probes per dispatching spin: {probes}");
            assert_eq!(
                probes, 0,
                "the spin that dispatched the XRCE arrival made {probes} `has_data` call(s); \
                 its readiness came from the entity's ready bit"
            );
            break;
        }
    }
    assert!(
        received.load(Ordering::SeqCst) >= 1,
        "the arrival was dispatched (the lower bound the zero above depends on)"
    );

    drop(publisher);
    let _ = exec.close();
    drop(agent);
}

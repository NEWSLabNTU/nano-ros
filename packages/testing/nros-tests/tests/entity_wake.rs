//! phase-397 W2 (RFC-0084) — an arrival names its entity and its owning
//! executor, end to end over zenoh.
//!
//! Each subscription the executor registers is armed with a per-entity waker
//! (`*_set_on_new_*_callback` across the cffi ABI). On arrival the zenoh shim
//! wakes that waker instead of the session's runtime wake, and the waker marks
//! the entry's slot in the executor's ready word. Two consequences are tested:
//!
//! 1. Three subscriptions on distinct topics: each arrival sets that
//!    subscription's bit and no other.
//! 2. Two executors over ONE session, the subscription on the first: its
//!    arrivals do not wake the second. Before W2 the session's wake fan-out
//!    (RFC-0106 D6) woke every member per arrival, which phase-436 step 5
//!    measured at 7-9 us of CPU per spurious wake.
//!
//! Run: `cargo nextest run -p nros-tests --features trigger-test --test entity_wake`.

#![cfg(feature = "trigger-test")]

// Force-link the zenoh-pico backend so its `.init_array` ctor registers the
// vtable before `Executor::open` runs.
use nros_rmw_zenoh as _;

use std::time::{Duration, Instant};

use nros_node::{QoSProfile, executor::*};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

const TYPE: &str = "std_msgs/msg/Empty";
/// A CDR encapsulation header and nothing else: a raw subscription takes any
/// bytes, and this keeps the payload trivially small.
const PAYLOAD: &[u8] = &[0, 1, 0, 0];
/// How long a published sample may take to arrive through the router.
const ARRIVAL_BUDGET: Duration = Duration::from_secs(5);
/// How long the test waits, after a bit appeared, for a WRONG bit to appear.
const SETTLE: Duration = Duration::from_millis(200);

fn open(locator: &str, name: &str, domain: u32) -> Executor<'static> {
    let config = ExecutorConfig::new(locator)
        .node_name(name)
        .domain_id(domain);
    Executor::open(&config).expect("Executor::open failed")
}

/// Register a raw subscription on `topic` and return the slot bit it armed.
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
        "subscribing to {topic} armed {armed:#x}: zenoh accepts an entity waker, so exactly \
         one new slot must be attributed"
    );
    armed
}

/// Publish on `publisher` until `exec`'s ready word is non-zero, then let it
/// settle and return everything it collected.
fn publish_until_marked(
    exec: &Executor<'static>,
    publisher: &nros_node::EmbeddedRawPublisher,
) -> u64 {
    let start = Instant::now();
    let mut bits = 0;
    while bits == 0 {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "no entity wake within {ARRIVAL_BUDGET:?} of publishing"
        );
        publisher.publish_raw(PAYLOAD).expect("publish");
        std::thread::sleep(Duration::from_millis(50));
        bits |= exec.take_entity_ready_bits();
    }
    std::thread::sleep(SETTLE);
    bits | exec.take_entity_ready_bits()
}

#[rstest]
fn each_arrival_marks_its_own_subscription(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut sub_exec = open(&locator, "entity_wake_sub", domain);
    let node = sub_exec
        .node_builder("entity_wake_sub")
        .build()
        .expect("node");
    let topics = ["/entity_wake/a", "/entity_wake/b", "/entity_wake/c"];
    let slots: Vec<u64> = topics
        .iter()
        .map(|t| subscribe(&mut sub_exec, node, t))
        .collect();

    // Published on the SAME session: zenoh-pico's local-subscriber delivery
    // (always on) runs the same arrival hook a remote sample does, and the
    // POSIX session pool holds one session per process.
    let publishers: Vec<_> = topics
        .iter()
        .map(|t| {
            sub_exec
                .node_mut(node)
                .create_publisher_raw(t, TYPE, "")
                .expect("publisher")
        })
        .collect();

    // Negative control: nothing has been published, so nothing is marked.
    std::thread::sleep(SETTLE);
    assert_eq!(
        sub_exec.take_entity_ready_bits(),
        0,
        "the ready word holds bits before any arrival, so a later non-zero word proves nothing"
    );

    for (i, publisher) in publishers.iter().enumerate() {
        let bits = publish_until_marked(&sub_exec, publisher);
        assert_eq!(
            bits, slots[i],
            "publishing on {} marked {bits:#x}; its subscription is slot {:#x}",
            topics[i], slots[i]
        );
    }
}

/// A raw publisher moved to the publishing thread.
struct SendPublisher(nros_node::EmbeddedRawPublisher);
// SAFETY: zenoh-pico's multi-threaded POSIX build serialises a put against its
// read task internally, which is what a backend thread publishing beside the
// executor relies on everywhere. The publisher is dropped by the thread before
// the session closes (`join` below precedes every executor drop).
unsafe impl Send for SendPublisher {}

#[rstest]
fn a_shared_session_member_is_not_woken_by_another_members_arrivals(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    // `owner` opens the session; `other` joins it (the tier-task shape), so
    // both are members of the session's wake fan-out.
    let mut owner = open(&locator, "entity_wake_owner", domain);
    let onode = owner
        .node_builder("entity_wake_owner")
        .build()
        .expect("node");
    subscribe(&mut owner, onode, "/entity_wake/owned");

    // SAFETY: `owner` outlives `other` (declared earlier, dropped later) and is
    // not moved while `other` exists.
    let mut other = unsafe { Executor::open_with_session_handle(owner.session_handle()) };
    let xnode = other
        .node_builder("entity_wake_other")
        .build()
        .expect("node");
    // Something to spin over, on a topic nobody publishes.
    subscribe(&mut other, xnode, "/entity_wake/unused");

    let publisher = owner
        .node_mut(onode)
        .create_publisher_raw("/entity_wake/owned", TYPE, "")
        .expect("publisher");

    // Publish every 50 ms from another thread while `other` waits. A fan-out
    // member would be woken by each of these.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_t = stop.clone();
    let publisher = SendPublisher(publisher);
    let publisher_thread = std::thread::spawn(move || {
        let publisher = publisher;
        while !stop_t.load(std::sync::atomic::Ordering::SeqCst) {
            publisher.0.publish_raw(PAYLOAD).expect("publish");
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    // The route is up once the owner sees an arrival.
    let start = Instant::now();
    while owner.take_entity_ready_bits() == 0 {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "the owner saw no arrival within {ARRIVAL_BUDGET:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let wait = Duration::from_millis(800);
    let start = Instant::now();
    other.spin_once(wait);
    let elapsed = start.elapsed();

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    publisher_thread.join().unwrap();

    assert!(
        elapsed >= wait - Duration::from_millis(50),
        "`other` returned from spin_once after {elapsed:?} of {wait:?}: an arrival for the \
         owner's subscription woke a member that owns nothing on that topic (or set its wake \
         flag earlier, while the route came up)"
    );
    // Control: the arrivals really happened while `other` waited, and reached
    // their owner.
    assert_ne!(
        owner.take_entity_ready_bits(),
        0,
        "the owner saw no arrival while `other` waited, so `other` staying asleep proves nothing"
    );
}

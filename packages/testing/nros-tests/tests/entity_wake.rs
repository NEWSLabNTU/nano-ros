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
//!
//! And the consumer (W3): a spin that dispatches an attributed arrival reads
//! its bit and calls no `has_data` at all.
//!
//! And the arrived-vs-present rule (W5): a bit is cleared when its entity is
//! DISPATCHED, never when its queue drains, so an arrival that races a drain
//! is never left undispatched. A spurious dispatch to an empty queue is fine.
//!
//! And what a full ring keeps (issue 1979): a burst into a subscription slower
//! than its publisher ends on the burst's NEWEST sample, and the samples the
//! ring could not hold are counted.
//!
//! And what an idle session owes the park (RFC-0113 D6, phase-521 W2): in
//! THREAD mode zenoh reports no deadline, so the caller's budget bounds it.
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
        publisher.publish_serialized(PAYLOAD).expect("publish");
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

/// RFC-0113 D6 (phase-521 W2) — an idle zenoh executor owes zenoh-pico no
/// deadline.
///
/// On Linux zenoh-pico runs in RFC-0110 THREAD mode: its own executor thread
/// sends the keep-alives and checks the lease. The session used to report a
/// constant 3333 ms anyway, so an idle `spin_once` with a longer budget parked
/// for 3.3 s and woke for nothing. Now the caller's budget bounds the park, and
/// the spin waits it out.
#[rstest]
fn an_idle_zenoh_session_does_not_bound_the_park(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let locator = zenohd_unique.locator();
    let mut exec = open(
        &locator,
        "idle_deadline",
        u32::from(nros_tests::unique_ros_domain_id()),
    );

    // Longer than the old 3333 ms constant, so the session would have won.
    let budget = Duration::from_secs(4);
    let start = Instant::now();
    exec.spin_once(budget);
    let waited = start.elapsed();

    let (bound_us, source) = exec.last_park();
    assert_eq!(
        source,
        WakeSourceId::CallerBudget,
        "an idle zenoh session bounded the park at {bound_us} us: in THREAD mode it owes the \
         executor no deadline (RFC-0113 D6)"
    );
    assert_eq!(bound_us, budget.as_micros() as u64);
    // A lower bound, so a spin that returned without waiting cannot pass.
    assert!(
        waited >= Duration::from_millis(3_500),
        "an idle spin_once({budget:?}) returned after {waited:?}: something still woke it before \
         the old 3.3 s deadline"
    );
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
            publisher.0.publish_serialized(PAYLOAD).expect("publish");
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

#[rstest]
fn a_pushed_spin_dispatches_without_probing_readiness(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut exec = open(&locator, "entity_wake_pushed", domain);
    let node = exec
        .node_builder("entity_wake_pushed")
        .build()
        .expect("node");
    let received = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let received_cb = received.clone();
    let before = exec.entity_attributed_slots();
    exec.node_mut(node)
        .subscription("/entity_wake/pushed")
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>()
        .build(move |_data: &[u8]| {
            received_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .expect("register subscription");
    assert_eq!(
        (exec.entity_attributed_slots() & !before).count_ones(),
        1,
        "the subscription was not attributed, so its readiness can only be probed"
    );
    let publisher = exec
        .node_mut(node)
        .create_publisher_raw("/entity_wake/pushed", TYPE, "")
        .expect("publisher");

    // The first spin after arming asks `has_data` once, for data that may
    // predate the registration. After that an idle spin asks nothing.
    exec.spin_once(Duration::from_millis(20));
    let idle_from = exec.readiness_probe_count();
    exec.spin_once(Duration::from_millis(20));
    assert_eq!(
        exec.readiness_probe_count() - idle_from,
        0,
        "an idle spin over one attributed subscription probed its readiness"
    );

    let start = Instant::now();
    let mut published = false;
    loop {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "the subscription dispatched nothing within {ARRIVAL_BUDGET:?} of publishing"
        );
        if !published || start.elapsed() > Duration::from_secs(1) {
            publisher.publish_serialized(PAYLOAD).expect("publish");
            published = true;
        }
        let probes_before = exec.readiness_probe_count();
        exec.spin_once(Duration::from_millis(100));
        let probes = exec.readiness_probe_count() - probes_before;
        if received.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            assert_eq!(
                probes, 0,
                "the spin that dispatched the arrival made {probes} `has_data` call(s); its \
                 readiness came from the entity's ready bit"
            );
            break;
        }
    }

    // The slot that was ready is asked once more (a dispatch may leave data
    // behind), then the executor goes quiet again.
    let after_from = exec.readiness_probe_count();
    exec.spin_once(Duration::from_millis(20));
    exec.spin_once(Duration::from_millis(20));
    assert!(
        exec.readiness_probe_count() - after_from <= 1,
        "after the dispatch the scan kept probing: {} call(s) over two spins",
        exec.readiness_probe_count() - after_from
    );
}

/// Publish once on `publisher` and spin `exec` until `done` holds or the
/// budget runs out.
fn spin_until(
    exec: &mut Executor<'static>,
    publisher: Option<&nros_node::EmbeddedRawPublisher>,
    budget: Duration,
    done: impl Fn() -> bool,
) -> bool {
    if let Some(p) = publisher {
        p.publish_serialized(PAYLOAD).expect("publish");
    }
    let start = Instant::now();
    while start.elapsed() < budget {
        exec.spin_once(Duration::from_millis(20));
        if done() {
            return true;
        }
    }
    false
}

#[rstest]
fn an_arrival_a_trigger_held_back_is_dispatched_when_it_passes(zenohd_unique: ZenohRouter) {
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut exec = open(&locator, "entity_wake_all", domain);
    let node = exec.node_builder("entity_wake_all").build().expect("node");
    let counts: [std::sync::Arc<AtomicU32>; 2] = Default::default();
    let topics = ["/entity_wake/all_a", "/entity_wake/all_b"];
    for (topic, count) in topics.iter().zip(&counts) {
        let count = count.clone();
        exec.node_mut(node)
            .subscription(topic)
            .generic(TYPE, "")
            .qos(QoSProfile::default())
            .rx_buffer::<256>()
            .build(move |_data: &[u8]| {
                count.fetch_add(1, SeqCst);
            })
            .expect("register subscription");
    }
    assert_eq!(exec.entity_attributed_slots().count_ones(), 2);
    let publishers: Vec<_> = topics
        .iter()
        .map(|t| {
            exec.node_mut(node)
                .create_publisher_raw(t, TYPE, "")
                .expect("publisher")
        })
        .collect();
    // Dispatch only when BOTH subscriptions have data.
    exec.set_trigger(Trigger::All);

    // `a` arrives alone: its bit is taken by a spin whose trigger fails, so
    // nothing dispatches, and the arrival must not be forgotten.
    let total = || counts[0].load(SeqCst) + counts[1].load(SeqCst);
    assert!(
        !spin_until(
            &mut exec,
            Some(&publishers[0]),
            Duration::from_millis(600),
            || { total() > 0 }
        ),
        "Trigger::All dispatched with only one subscription holding data"
    );

    // `b` arrives: now both hold data, so both dispatch, `a` included,
    // although no arrival has marked `a` since its bit was taken.
    assert!(
        spin_until(&mut exec, Some(&publishers[1]), ARRIVAL_BUDGET, || {
            counts[0].load(SeqCst) > 0 && counts[1].load(SeqCst) > 0
        }),
        "after both arrivals: a dispatched {} time(s), b {} time(s); an arrival taken by a \
         spin whose trigger failed was lost",
        counts[0].load(SeqCst),
        counts[1].load(SeqCst)
    );
}

/// The two dispatch shapes a raw subscription can take.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// In-place: one dispatch drains the backend's queue to empty, so an
    /// arrival during its callback is usually taken by the same dispatch.
    DrainsToEmpty,
    /// `.message_info()`: one dispatch takes ONE sample, so an arrival during
    /// its callback is still queued when the dispatch ends. This is the shape
    /// the dispatch-clears rule protects.
    OneSamplePerDispatch,
}

#[rstest]
#[case::drains_to_empty(Shape::DrainsToEmpty)]
#[case::one_sample_per_dispatch(Shape::OneSamplePerDispatch)]
fn an_arrival_racing_a_drain_is_never_left_undispatched(
    zenohd_unique: ZenohRouter,
    #[case] shape: Shape,
) {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
    /// Long enough for the publisher to see the callback running and publish
    /// inside it.
    const CALLBACK_COST: Duration = Duration::from_millis(3);
    /// Longer than the callback, so the backend's ring (depth 4 by default)
    /// never fills and this test stays about dispatch. What a full ring
    /// keeps is the next test's subject (issue 1979).
    const PUBLISH_INTERVAL: Duration = Duration::from_millis(5);
    const ROUNDS: u32 = 20;
    const BURST: u32 = 3;
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut exec = open(&locator, "entity_wake_race", domain);
    let node = exec.node_builder("entity_wake_race").build().expect("node");
    // The highest sequence number dispatched, and whether a callback is
    // running right now.
    let latest = std::sync::Arc::new(AtomicU32::new(0));
    let in_callback = std::sync::Arc::new(AtomicBool::new(false));
    let (latest_cb, in_callback_cb) = (latest.clone(), in_callback.clone());
    let on_sample = move |data: &[u8]| {
        in_callback_cb.store(true, SeqCst);
        if let Some(seq) = data.get(4..8) {
            latest_cb.fetch_max(u32::from_le_bytes(seq.try_into().unwrap()), SeqCst);
        }
        std::thread::sleep(CALLBACK_COST);
        in_callback_cb.store(false, SeqCst);
    };
    let mut handle = exec.node_mut(node);
    let builder = handle
        .subscription("/entity_wake/race")
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>();
    match shape {
        Shape::DrainsToEmpty => builder.build(on_sample),
        Shape::OneSamplePerDispatch => builder
            .message_info()
            .build(move |data: &[u8], _info: &_| on_sample(data)),
    }
    .expect("register subscription");
    assert_eq!(
        exec.entity_attributed_slots().count_ones(),
        1,
        "the subscription was not attributed, so this would test the scan, not the bits"
    );
    let publisher = SendPublisher(
        exec.node_mut(node)
            .create_publisher_raw("/entity_wake/race", TYPE, "")
            .expect("publisher"),
    );

    // Each round publishes a burst from another thread while this one spins,
    // and holds the burst's LAST sample until a callback is running, so it
    // arrives while the executor is dispatching that entity. No later arrival
    // announces it again: if the executor drops an arrival that lands during
    // the dispatch, that sample is never dispatched.
    //
    // Issue 1980: a publish that FAILS says nothing about dispatch, so it is
    // reported as a publish failure, not as a lost arrival. It used to fail
    // under CPU load (1 run in 25): zenoh-pico's default DROP congestion
    // control dropped the put whenever another thread held the transport's TX
    // mutex. Since issue 2001 a RELIABLE publisher waits for the mutex
    // instead, so this test no longer retries — a publish failure here is a
    // regression of 2001.
    let (to_publisher, rounds) = std::sync::mpsc::channel::<u32>();
    let (raced_tx, raced) = std::sync::mpsc::channel::<Result<bool, String>>();
    let in_callback_pub = in_callback.clone();
    let publisher_thread = std::thread::spawn(move || {
        let publisher = publisher;
        let publish = |seq: u32| -> Result<(), String> {
            let mut payload = [0u8, 1, 0, 0, 0, 0, 0, 0];
            payload[4..8].copy_from_slice(&seq.to_le_bytes());
            publisher
                .0
                .publish_serialized(&payload)
                .map_err(|e| format!("sample {seq}: publish failed: {e:?} (issue 2001)"))
        };
        while let Ok(first) = rounds.recv() {
            let last = first + BURST - 1;
            let outcome = (|| {
                for seq in first..last {
                    if seq > first {
                        std::thread::sleep(PUBLISH_INTERVAL);
                    }
                    publish(seq)?;
                }
                // Wait for that sample's dispatch to start, then publish into it.
                let start = Instant::now();
                while !in_callback_pub.load(SeqCst) && start.elapsed() < ARRIVAL_BUDGET {
                    std::hint::spin_loop();
                }
                let raced = in_callback_pub.load(SeqCst);
                publish(last)?;
                Ok(raced)
            })();
            raced_tx.send(outcome).unwrap();
        }
    });

    let mut raced_rounds = 0;
    for round in 0..ROUNDS {
        let first = 1 + round * BURST;
        let last = first + BURST - 1;
        to_publisher.send(first).unwrap();
        // The arrival budget runs from the LAST sample's publish, not from the
        // round's start: the publisher may wait up to `ARRIVAL_BUDGET` itself
        // for a callback to publish into, and a budget that started earlier
        // ran out during that wait and blamed dispatch for a sample not yet
        // sent (issue 1980, measured 1 run in 25 under CPU load).
        let round_start = Instant::now();
        let mut published_at = None;
        let mut raced_this_round = None;
        while latest.load(SeqCst) < last {
            if raced_this_round.is_none() {
                raced_this_round = raced.try_recv().ok();
                if raced_this_round.is_some() {
                    published_at = Some(Instant::now());
                }
            }
            if let Some(Err(failure)) = &raced_this_round {
                panic!(
                    "{shape:?} round {round}: {failure}. The sample was never sent, so this \
                     says nothing about dispatch (issue 1980)"
                );
            }
            assert!(
                round_start.elapsed() < ARRIVAL_BUDGET * 2,
                "{shape:?} round {round}: the publisher never reported sample {last} \
                 (latest dispatched {})",
                latest.load(SeqCst)
            );
            assert!(
                published_at.is_none_or(|at: Instant| at.elapsed() < ARRIVAL_BUDGET),
                "{shape:?} round {round}: sample {last}, published while a callback ran \
                 ({:?}), was never dispatched (latest {}); an arrival that raced a dispatch \
                 was lost",
                raced_this_round,
                latest.load(SeqCst)
            );
            exec.spin_once(Duration::from_millis(50));
        }
        let raced_this_round = match raced_this_round {
            Some(r) => r,
            None => raced
                .recv_timeout(ARRIVAL_BUDGET)
                .expect("publisher finished"),
        }
        .unwrap_or_else(|failure| panic!("{shape:?} round {round}: {failure}"));
        raced_rounds += u32::from(raced_this_round);
    }
    drop(to_publisher);
    publisher_thread.join().unwrap();
    // Control: the race this test exists for really happened.
    assert!(
        raced_rounds >= ROUNDS / 2,
        "only {raced_rounds} of {ROUNDS} last samples were published while a callback ran, \
         so the race was not exercised"
    );
}

/// Issue 1979 — a burst into a slow subscription fills the backend's ring
/// (depth 4 by default). The ring is KEEP_LAST, so the burst's final sample
/// must be dispatched, whichever dispatch shape the subscription has. The
/// ring used to refuse the incoming sample instead, and this measured
/// `latest 23` of 25 with the same shape. The samples the ring could not
/// hold are counted, which is also the control that the ring did fill.
///
/// Issue 1997 — the ring fills BY CONSTRUCTION, not by speed. The burst used
/// to come from a publisher thread that had to outrun a deliberately slow
/// callback, and under load (or, after issue 2001 slowed the publisher, even
/// alone) it often did not, so the control refused the run. Now the "slow
/// subscription" is one nobody drains: the whole burst is published while
/// the executor is NOT spinning, so zenoh-pico's background read task lands
/// every arrival in a ring that nothing empties. That the ring overflowed is
/// then a precondition, waited for and asserted before any dispatch, and the
/// spin afterwards tests only what a full ring kept.
#[rstest]
#[case::drains_to_empty(Shape::DrainsToEmpty)]
#[case::one_sample_per_dispatch(Shape::OneSamplePerDispatch)]
fn a_burst_into_a_slow_subscription_ends_on_its_newest_sample(
    zenohd_unique: ZenohRouter,
    #[case] shape: Shape,
) {
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    /// Well past the ring's depth (4), so most of the burst must be evicted.
    const BURST: u32 = 25;
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut exec = open(&locator, "entity_wake_burst", domain);
    let node = exec
        .node_builder("entity_wake_burst")
        .build()
        .expect("node");
    let latest = std::sync::Arc::new(AtomicU32::new(0));
    let latest_cb = latest.clone();
    let on_sample = move |data: &[u8]| {
        if let Some(seq) = data.get(4..8) {
            latest_cb.fetch_max(u32::from_le_bytes(seq.try_into().unwrap()), SeqCst);
        }
    };
    let mut handle = exec.node_mut(node);
    let builder = handle
        .subscription("/entity_wake/burst")
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>();
    match shape {
        Shape::DrainsToEmpty => builder.build(on_sample),
        Shape::OneSamplePerDispatch => builder
            .message_info()
            .build(move |data: &[u8], _info: &_| on_sample(data)),
    }
    .expect("register subscription");
    let publisher = exec
        .node_mut(node)
        .create_publisher_raw("/entity_wake/burst", TYPE, "")
        .expect("publisher");
    let publish = |seq: u32| {
        let mut payload = [0u8, 1, 0, 0, 0, 0, 0, 0];
        payload[4..8].copy_from_slice(&seq.to_le_bytes());
        publisher
            .publish_serialized(&payload)
            .unwrap_or_else(|e| panic!("{shape:?}: sample {seq}: publish failed: {e:?}"));
    };

    // The first sample, alone, until it is dispatched: discovery has settled
    // and the burst cannot be lost before the subscription matches.
    let start = Instant::now();
    while latest.load(SeqCst) < 1 {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "{shape:?}: the first sample never arrived"
        );
        publish(1);
        exec.spin_once(Duration::from_millis(50));
    }
    let drops_before = nros_rmw_zenoh::ring_full_drops_total();

    // The burst, with the executor NOT spinning: nothing drains the ring, so
    // zenoh-pico's read task must overflow it. Waiting for that (without a
    // spin) makes the eviction a precondition rather than a race.
    for seq in 2..=BURST {
        publish(seq);
    }
    let start = Instant::now();
    while nros_rmw_zenoh::ring_full_drops_total() <= drops_before {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "{shape:?}: {} samples published into an undrained ring of depth 4 and none \
             was evicted within {ARRIVAL_BUDGET:?}; either the arrivals never landed or the \
             ring is deeper than this test assumes, so it cannot test eviction",
            BURST - 1
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // Let the rest of the burst land too, so the spin below sees the ring's
    // final contents, not a burst still in flight.
    let settle = Instant::now();
    let mut drops = nros_rmw_zenoh::ring_full_drops_total();
    while settle.elapsed() < ARRIVAL_BUDGET {
        std::thread::sleep(Duration::from_millis(20));
        let now = nros_rmw_zenoh::ring_full_drops_total();
        if now == drops {
            break;
        }
        drops = now;
    }

    let start = Instant::now();
    while latest.load(SeqCst) < BURST && start.elapsed() < ARRIVAL_BUDGET {
        exec.spin_once(Duration::from_millis(50));
    }
    assert_eq!(
        latest.load(SeqCst),
        BURST,
        "{shape:?}: the burst's newest sample was never dispatched; a full ring kept older \
         samples instead"
    );
    // Control: the ring really filled, so this tested what a full ring keeps.
    // Asserted above as a precondition; kept here so the verdict names both.
    assert!(
        nros_rmw_zenoh::ring_full_drops_total() > drops_before,
        "{shape:?}: the ring never filled, so the burst did not test eviction"
    );
}

/// This thread's CPU time (user + system), from `getrusage(RUSAGE_THREAD)`.
fn thread_cpu() -> Duration {
    // SAFETY: `getrusage` writes the struct it is handed and nothing else.
    let mut usage: libc::rusage = unsafe { core::mem::zeroed() };
    let rc = unsafe { libc::getrusage(libc::RUSAGE_THREAD, &mut usage) };
    assert_eq!(rc, 0, "getrusage(RUSAGE_THREAD) failed");
    let tv = |t: libc::timeval| {
        Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64)
    };
    tv(usage.ru_utime) + tv(usage.ru_stime)
}

/// phase-397 W6 — what an idle shared-session member costs while another
/// member's subscription takes ~1 kHz of arrivals. phase-436 step 5 measured
/// the fan-out this phase removed: every idle member spun once per arrival,
/// 999-1000 times a second, at 6.8-9.4 us of thread CPU each (0.7-0.9 % of a
/// core). With entity wakes the member sleeps out each park. The numbers are
/// printed for the phase doc; the assertion is the bound that separates the
/// two behaviours by two orders of magnitude.
#[rstest]
fn an_idle_member_parks_through_kilohertz_arrivals_for_another(zenohd_unique: ZenohRouter) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
    };
    const RUN: Duration = Duration::from_secs(3);
    const PARK: Duration = Duration::from_millis(500);
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut owner = open(&locator, "entity_wake_khz_owner", domain);
    let onode = owner
        .node_builder("entity_wake_khz_owner")
        .build()
        .expect("node");
    subscribe(&mut owner, onode, "/entity_wake/khz");
    // SAFETY: `owner` outlives `other` (declared earlier, dropped later) and is
    // not moved while `other` exists.
    let mut other = unsafe { Executor::open_with_session_handle(owner.session_handle()) };
    let xnode = other
        .node_builder("entity_wake_khz_idle")
        .build()
        .expect("node");
    subscribe(&mut other, xnode, "/entity_wake/khz_unused");
    let publisher = SendPublisher(
        owner
            .node_mut(onode)
            .create_publisher_raw("/entity_wake/khz", TYPE, "")
            .expect("publisher"),
    );

    let stop = Arc::new(AtomicBool::new(false));
    let published = Arc::new(AtomicU64::new(0));
    let (stop_t, published_t) = (stop.clone(), published.clone());
    let publisher_thread = std::thread::spawn(move || {
        let publisher = publisher;
        while !stop_t.load(SeqCst) {
            publisher.0.publish_serialized(PAYLOAD).expect("publish");
            published_t.fetch_add(1, SeqCst);
            std::thread::sleep(Duration::from_millis(1));
        }
    });

    let start = Instant::now();
    while owner.take_entity_ready_bits() == 0 {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "the owner saw no arrival within {ARRIVAL_BUDGET:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let published_from = published.load(SeqCst);
    let cpu_from = thread_cpu();
    let start = Instant::now();
    let mut spins = 0u32;
    while start.elapsed() < RUN {
        other.spin_once(PARK);
        spins += 1;
    }
    let elapsed = start.elapsed();
    let cpu = thread_cpu() - cpu_from;
    let arrivals = published.load(SeqCst) - published_from;

    stop.store(true, SeqCst);
    publisher_thread.join().unwrap();

    let rate = arrivals as f64 / elapsed.as_secs_f64();
    eprintln!(
        "phase-397 W6: idle member over {elapsed:?}: {spins} spins ({:.1}/s), {cpu:?} thread CPU \
         ({:.3} % of a core), while another member took {rate:.0} arrivals/s",
        f64::from(spins) / elapsed.as_secs_f64(),
        cpu.as_secs_f64() * 100.0 / elapsed.as_secs_f64(),
    );
    // Control: the arrivals really came at a rate that would have woken a
    // fan-out member hundreds of times a second.
    assert!(
        rate >= 300.0,
        "only {rate:.0} arrivals/s were published, too few to tell a fan-out member from a \
         parked one"
    );
    assert_ne!(
        owner.take_entity_ready_bits(),
        0,
        "the owner saw no arrival while `other` spun, so `other` staying parked proves nothing"
    );
    let bound = (RUN.as_millis() / PARK.as_millis()) as u32 + 2;
    assert!(
        spins <= bound,
        "the idle member spun {spins} times in {elapsed:?} (bound {bound}, one per {PARK:?} \
         park): another member's arrivals are waking it"
    );
}

/// phase-397 W6 — arrival-to-callback latency for an attributed subscription,
/// with a park far longer than the gap between samples. A dispatch that waited
/// for the park to end would put the median in the hundreds of milliseconds;
/// a pushed wake puts it at the router's round trip. Printed for the phase doc.
#[rstest]
fn an_attributed_arrival_reaches_its_callback_without_waiting_out_the_park(
    zenohd_unique: ZenohRouter,
) {
    use std::sync::{Arc, Mutex};
    const SAMPLES: usize = 200;
    const GAP: Duration = Duration::from_millis(10);
    const PARK: Duration = Duration::from_millis(1000);
    require_zenohd();
    let locator = zenohd_unique.locator();
    let domain = u32::from(nros_tests::unique_ros_domain_id());

    let mut exec = open(&locator, "entity_wake_latency", domain);
    let node = exec
        .node_builder("entity_wake_latency")
        .build()
        .expect("node");
    let sent: Arc<Mutex<Vec<Option<Instant>>>> = Arc::new(Mutex::new(vec![None; SAMPLES + 1]));
    let got: Arc<Mutex<Vec<Option<Instant>>>> = Arc::new(Mutex::new(vec![None; SAMPLES + 1]));
    let got_cb = got.clone();
    exec.node_mut(node)
        .subscription("/entity_wake/latency")
        .generic(TYPE, "")
        .qos(QoSProfile::default())
        .rx_buffer::<256>()
        .build(move |data: &[u8]| {
            let now = Instant::now();
            if let Some(seq) = data.get(4..8) {
                let seq = u32::from_le_bytes(seq.try_into().unwrap()) as usize;
                if let Some(slot) = got_cb.lock().unwrap().get_mut(seq) {
                    slot.get_or_insert(now);
                }
            }
        })
        .expect("register subscription");
    let publisher = exec
        .node_mut(node)
        .create_publisher_raw("/entity_wake/latency", TYPE, "")
        .expect("publisher");
    let payload = |seq: u32| {
        let mut p = [0u8, 1, 0, 0, 0, 0, 0, 0];
        p[4..8].copy_from_slice(&seq.to_le_bytes());
        p
    };

    // Sample 0 until it is dispatched: the route is up before timing starts.
    let start = Instant::now();
    while got.lock().unwrap()[0].is_none() {
        assert!(
            start.elapsed() < ARRIVAL_BUDGET,
            "the warm-up sample never arrived"
        );
        publisher.publish_serialized(&payload(0)).expect("publish");
        exec.spin_once(Duration::from_millis(50));
    }

    let publisher = SendPublisher(publisher);
    let sent_t = sent.clone();
    let publisher_thread = std::thread::spawn(move || {
        let publisher = publisher;
        for seq in 1..=SAMPLES as u32 {
            sent_t.lock().unwrap()[seq as usize] = Some(Instant::now());
            publisher
                .0
                .publish_serialized(&payload(seq))
                .expect("publish");
            std::thread::sleep(GAP);
        }
    });
    let budget = GAP * SAMPLES as u32 + ARRIVAL_BUDGET;
    let start = Instant::now();
    let received = || got.lock().unwrap()[1..].iter().flatten().count();
    while received() < SAMPLES && start.elapsed() < budget {
        exec.spin_once(PARK);
    }
    publisher_thread.join().unwrap();

    let (sent, got) = (sent.lock().unwrap(), got.lock().unwrap());
    let mut latencies: Vec<Duration> = (1..=SAMPLES)
        .filter_map(|i| Some(got[i]?.saturating_duration_since(sent[i]?)))
        .collect();
    latencies.sort_unstable();
    let n = latencies.len();
    assert!(
        n >= SAMPLES * 9 / 10,
        "only {n} of {SAMPLES} samples were dispatched"
    );
    let (median, p90, max) = (latencies[n / 2], latencies[n * 9 / 10], latencies[n - 1]);
    eprintln!(
        "phase-397 W6: arrival-to-callback over {n} samples, {PARK:?} park: median {median:?}, \
         p90 {p90:?}, max {max:?}"
    );
    assert!(
        median < Duration::from_millis(20),
        "median arrival-to-callback {median:?} with a {PARK:?} park: dispatch waited for the \
         park instead of the arrival's wake"
    );
}

/// phase-521 W7 (RFC-0113 D2) — the Linux witness: an idle zenoh entry wakes
/// only for its deadlines, and wakes ON them.
///
/// The executor's entry budget for a THREAD-mode zenoh session with no ticking
/// component is the idle budget (60 s), so a loop of entry spins over 3 s with
/// one 1 s timer returns about once per timer expiry. With the old fixed 10 ms
/// budget it returned ~300 times. Each timer dispatch lands within 50 ms of its
/// deadline (Linux parks on a 1 us-granularity `sem_clockwait`; the margin is
/// for scheduling under a loaded test host).
#[rstest]
fn an_idle_zenoh_entry_wakes_only_for_its_deadlines(zenohd_unique: ZenohRouter) {
    use std::sync::{Arc, Mutex};
    require_zenohd();
    let locator = zenohd_unique.locator();
    let mut exec = open(
        &locator,
        "idle_entry",
        u32::from(nros_tests::unique_ros_domain_id()),
    );
    assert_eq!(
        exec.entry_spin_budget(),
        nros_node::ENTRY_IDLE_BUDGET,
        "an idle THREAD-mode zenoh executor with no ticking component must park past the slice"
    );

    let fired: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = fired.clone();
    let start = Instant::now();
    exec.register_timer(nros_node::TimerDuration::from_millis(1000), move || {
        sink.lock().unwrap().push(Instant::now());
    })
    .expect("register_timer");

    const WINDOW: Duration = Duration::from_millis(3_500);
    let mut spins = 0u32;
    while start.elapsed() < WINDOW {
        let budget = exec
            .entry_spin_budget()
            .min(WINDOW.saturating_sub(start.elapsed()));
        exec.spin_once(budget);
        spins += 1;
    }
    let fired = fired.lock().unwrap().clone();

    assert!(
        (3..=4).contains(&fired.len()),
        "a 1 s timer fired {} times in 3.5 s",
        fired.len()
    );
    // One wake per expiry, plus the loop's own last partial park; a 10 ms
    // budget would have spun ~350 times.
    assert!(
        spins <= 8,
        "an idle entry spun {spins} times in {WINDOW:?} with one 1 s timer: it is \
         waking on a slice, not on its deadlines"
    );
    for (i, at) in fired.iter().enumerate() {
        let due = Duration::from_millis(1000 * (i as u64 + 1));
        let late = at.duration_since(start).saturating_sub(due);
        let early = due.saturating_sub(at.duration_since(start));
        assert!(
            early < Duration::from_millis(5) && late < Duration::from_millis(50),
            "timer expiry {i} dispatched at {:?}, due at {due:?}",
            at.duration_since(start)
        );
    }
    let at: Vec<Duration> = fired.iter().map(|t| t.duration_since(start)).collect();
    eprintln!("idle zenoh entry: {spins} spins in {WINDOW:?}, timer dispatched at {at:?}");
}

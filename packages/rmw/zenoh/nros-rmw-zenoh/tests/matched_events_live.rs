//! Issue 1972 — matched and incompatible-type events against a LIVE peer.
//!
//! The zenoh shim derives `SubscriptionMatched` / `PublicationMatched` from its
//! graph cache (the standing liveliness subscriber), the way `rmw_zenoh_cpp`
//! does. The only honest test of that is a peer whose liveliness token really
//! arrives and really leaves, so the peer is a SECOND PROCESS: this same test
//! binary re-executed with `NROS_MATCHED_PEER` set, opening its own session —
//! the shim holds one session per process, and a token our own session declared
//! would prove nothing about the wire.
//!
//! Asserted:
//!  * a subscription's `current_count` goes 0 -> 1 when a same-type publisher
//!    appears and 1 -> 0 when its process is killed;
//!  * a publisher sees `PublicationMatched` when a same-type subscription
//!    appears (observed on its poll point, `assert_liveliness`);
//!  * a publisher of ANOTHER type on the same topic name raises
//!    `SubscriptionIncompatibleType` and does not count as matched.
//!
//! Needs `rmw_zenohd` (an absent router is an unmet precondition, not a pass).
//! Run with `just linux test-matched-events`.

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use nros_rmw::{
    EventKind, Publisher, QoSProfile, Session, SessionMode, Subscription, TopicInfo, Transport,
    TransportConfig,
};
use nros_rmw_zenoh::ZenohTransport;
use nros_tests::fixtures::ZenohRouter;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const TOPIC: &str = "/matched_events_live";
const TYPE_SAME: &str = "std_msgs::msg::dds_::String_";
const TYPE_OTHER: &str = "std_msgs::msg::dds_::Int32_";
const PEER_ENV: &str = "NROS_MATCHED_PEER";
const LOCATOR_ENV: &str = "NROS_MATCHED_PEER_LOCATOR";
const READY: &str = "MATCHED_PEER_READY";

/// Everything the callbacks saw, in order.
static EVENTS: Mutex<Vec<(EventKind, u32, i32, u32)>> = Mutex::new(Vec::new());

unsafe extern "C" fn record(kind: EventKind, payload: *const c_void, _ctx: *mut c_void) {
    let row = match unsafe { nros_rmw::payload_from_raw(kind, payload) } {
        nros_rmw::EventPayload::SubscriptionMatched(s)
        | nros_rmw::EventPayload::PublicationMatched(s) => {
            (kind, s.current_count, s.current_count_change, s.total_count)
        }
        nros_rmw::EventPayload::SubscriptionIncompatibleType(s)
        | nros_rmw::EventPayload::PublisherIncompatibleType(s) => (
            kind,
            s.total_count,
            s.total_count_change as i32,
            s.total_count,
        ),
        other => panic!("unexpected payload {other:?}"),
    };
    EVENTS.lock().unwrap().push(row);
}

fn open(locator: &str) -> nros_rmw_zenoh::ZenohSession {
    let config = TransportConfig {
        locator: Some(locator),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "matched_events_live",
        namespace: "",
        domain_id: 0,
    };
    ZenohTransport::open(&config).expect("open zenoh session")
}

/// The peer process: declare one endpoint, say so, and live until killed.
fn run_peer(mode: &str) -> ! {
    let locator = std::env::var(LOCATOR_ENV).expect("peer locator");
    let mut sess = open(&locator);
    let (ty, publisher) = match mode {
        "pub-same" => (TYPE_SAME, true),
        "pub-other" => (TYPE_OTHER, true),
        "sub-same" => (TYPE_SAME, false),
        other => panic!("unknown peer mode {other}"),
    };
    // A node name, or the entity declares no liveliness token at all.
    let topic = TopicInfo::new(TOPIC, ty, "RIHS01_unused").with_node_name("matched_peer");
    let _pub;
    let _sub;
    if publisher {
        _pub = sess
            .create_publisher(&topic, QoSProfile::QOS_PROFILE_DEFAULT)
            .expect("peer publisher");
    } else {
        _sub = sess
            .create_subscription(&topic, QoSProfile::QOS_PROFILE_DEFAULT)
            .expect("peer subscription");
    }
    println!("{READY}");
    loop {
        let _ = sess.drive_io(100);
    }
}

/// A peer process that is killed when dropped.
struct Peer(Child);

impl Peer {
    fn spawn(mode: &str, locator: &str) -> Self {
        let exe = std::env::current_exe().expect("current_exe");
        let mut child = Command::new(exe)
            .args([
                "--exact",
                "zenoh_matched_events_against_a_live_peer",
                "--nocapture",
            ])
            .env(PEER_ENV, mode)
            .env(LOCATOR_ENV, locator)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn peer");
        let stdout = child.stdout.take().expect("peer stdout");
        let mut lines = BufReader::new(stdout).lines();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(
                Instant::now() < deadline,
                "peer `{mode}` never became ready"
            );
            match lines.next() {
                Some(Ok(l)) if l.contains(READY) => break,
                Some(Ok(_)) => continue,
                _ => panic!("peer `{mode}` exited before it was ready"),
            }
        }
        Peer(child)
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Poll `poll` until an event of `kind` satisfying `want` is recorded.
fn wait_for(
    kind: EventKind,
    poll: &mut dyn FnMut(),
    want: impl Fn(&(EventKind, u32, i32, u32)) -> bool,
    what: &str,
) -> (EventKind, u32, i32, u32) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        poll();
        if let Some(row) = EVENTS
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.0 == kind && want(r))
            .copied()
        {
            return row;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; saw {:?}",
            EVENTS.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(60));
    }
}

#[test]
fn zenoh_matched_events_against_a_live_peer() {
    if let Ok(mode) = std::env::var(PEER_ENV) {
        run_peer(&mode);
    }
    let router = match ZenohRouter::start_unique() {
        Ok(r) => r,
        Err(e) => nros_tests::unmet!(
            "{}",
            nros_tests::process::zenohd_unavailable_reason()
                .unwrap_or_else(|| format!("could not start rmw_zenohd: {e:?}"))
        ),
    };
    let locator = router.locator();
    let mut sess = open(&locator);

    let topic = TopicInfo::new(TOPIC, TYPE_SAME, "RIHS01_unused").with_node_name("matched_main");
    let mut sub = sess
        .create_subscription(&topic, QoSProfile::QOS_PROFILE_DEFAULT)
        .expect("create_subscription");
    assert!(sub.supports_event(EventKind::SubscriptionMatched));
    assert!(sub.supports_event(EventKind::SubscriptionIncompatibleType));
    unsafe {
        sub.register_event_callback(
            EventKind::SubscriptionMatched,
            0,
            record,
            core::ptr::null_mut(),
        )
    }
    .expect("register SubscriptionMatched");
    unsafe {
        sub.register_event_callback(
            EventKind::SubscriptionIncompatibleType,
            0,
            record,
            core::ptr::null_mut(),
        )
    }
    .expect("register SubscriptionIncompatibleType");

    // ---- nobody yet: no event --------------------------------------------
    let settle = Instant::now() + Duration::from_millis(800);
    while Instant::now() < settle {
        let _ = sess.drive_io(20);
        let _ = sub.has_data();
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(
        EVENTS.lock().unwrap().is_empty(),
        "an event with no peer: {:?}",
        EVENTS.lock().unwrap()
    );

    // ---- 0 -> 1: a same-type publisher appears ---------------------------
    let peer = Peer::spawn("pub-same", &locator);
    let up = wait_for(
        EventKind::SubscriptionMatched,
        &mut || {
            let _ = sess.drive_io(20);
            let _ = sub.has_data();
        },
        |r| r.1 == 1,
        "SubscriptionMatched current_count = 1",
    );
    assert_eq!((up.1, up.2, up.3), (1, 1, 1), "0 -> 1 report");

    // ---- 1 -> 0: its process dies ----------------------------------------
    drop(peer);
    let down = wait_for(
        EventKind::SubscriptionMatched,
        &mut || {
            let _ = sess.drive_io(20);
            let _ = sub.has_data();
        },
        |r| r.1 == 0,
        "SubscriptionMatched current_count = 0",
    );
    assert_eq!((down.1, down.2, down.3), (0, -1, 1), "1 -> 0 report");

    // ---- another type on the same name: incompatible, not matched --------
    let other = Peer::spawn("pub-other", &locator);
    let it = wait_for(
        EventKind::SubscriptionIncompatibleType,
        &mut || {
            let _ = sess.drive_io(20);
            let _ = sub.has_data();
        },
        |_| true,
        "SubscriptionIncompatibleType",
    );
    assert_eq!((it.1, it.2), (1, 1), "one incompatible publisher");
    assert!(
        !EVENTS
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.0 == EventKind::SubscriptionMatched && r.1 > 0 && r.3 > 1),
        "a different-type publisher counted as matched: {:?}",
        EVENTS.lock().unwrap()
    );
    drop(other);

    // ---- publisher side: a same-type subscription appears ----------------
    let pub_topic =
        TopicInfo::new(TOPIC, TYPE_SAME, "RIHS01_unused").with_node_name("matched_main");
    let mut publisher = sess
        .create_publisher(&pub_topic, QoSProfile::QOS_PROFILE_DEFAULT)
        .expect("create_publisher");
    assert!(publisher.supports_event(EventKind::PublicationMatched));
    unsafe {
        publisher.register_event_callback(
            EventKind::PublicationMatched,
            0,
            record,
            core::ptr::null_mut(),
        )
    }
    .expect("register PublicationMatched");
    // Our own subscription on this topic may already count (whether this
    // session's cache sees its OWN tokens is the router's business), so the
    // claim is about the remote one: the count rises by exactly one.
    let settle = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < settle {
        let _ = sess.drive_io(20);
        let _ = publisher.assert_liveliness();
        std::thread::sleep(Duration::from_millis(40));
    }
    let baseline = EVENTS
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|r| r.0 == EventKind::PublicationMatched)
        .map_or(0, |r| r.1);
    let _sub_peer = Peer::spawn("sub-same", &locator);
    let pm = wait_for(
        EventKind::PublicationMatched,
        &mut || {
            let _ = sess.drive_io(20);
            // The publisher's poll point for a caller that is not publishing.
            let _ = publisher.assert_liveliness();
        },
        |r| r.1 == baseline + 1,
        "PublicationMatched with the remote subscription",
    );
    assert_eq!(pm.2, 1, "one subscription appeared: {pm:?}");
}

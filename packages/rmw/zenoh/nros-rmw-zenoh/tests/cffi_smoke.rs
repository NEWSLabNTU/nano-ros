//! Phase 115.L.2 — smoke + pubsub round-trip for zenoh-pico via cffi.

#![cfg(feature = "platform-posix")]

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use nros_rmw::{Publisher, QoSProfile, Session, Subscription, TopicInfo};
use nros_rmw_cffi::{CffiSession, NROS_RMW_RET_OK, RustBackendAdapter};
use nros_rmw_zenoh::ZenohRmw;
use nros_tests::fixtures::ZenohRouter;

#[test]
fn zenoh_cffi_register_returns_ok() {
    let rc = nros_rmw_zenoh::register();
    assert!(rc.is_ok(), "register failed: {rc:?}");
}

#[test]
fn zenoh_vtable_monomorphised_with_every_slot() {
    // Probe the monomorphised vtable to confirm none of the fn
    // pointers ended up null. Each entry is a real fn pointer
    // address so it's inherently non-null; the smoke check is
    // that this all *type-checks* with `ZenohRmw` filling the
    // `RustBackend` bundle.
    // RFC-0054: vtable slots are now `Option<fn>` (generated from the C
    // header's nullable fn pointers) — assert the adapter filled each one.
    let vt = &RustBackendAdapter::<ZenohRmw>::VTABLE;
    assert!(vt.create_session.is_some());
    assert!(vt.destroy_session.is_some());
    assert!(vt.drive_io.is_some());
    assert!(vt.create_publisher.is_some());
    assert!(vt.create_subscription.is_some());
    assert!(vt.create_service.is_some());
    assert!(vt.create_client.is_some());
    assert_eq!(NROS_RMW_RET_OK, 0);
}

// ----------------------------------------------------------------------------
// Pubsub round-trip via a one-shot zenohd fixture.
// ----------------------------------------------------------------------------

/// One zenohd per test run, shared across tests via OnceLock + Mutex.
/// Returns the locator string the tests should connect to. `None` if
/// the zenohd binary isn't built (caller should skip).
///
/// Issue 0573 — this used to be a private `RouterHandle` that spawned zenohd
/// with a bare `Command::spawn()`. That copy leaked a router on EVERY run: it
/// armed no `PR_SET_PDEATHSIG`, so nextest's SIGKILL left an orphan, and its
/// `impl Drop` was dead code because Rust never drops a `static`. Eleven such
/// routers were found alive on a dev host, the oldest 3.8 days old. It also
/// re-introduced the bind-port-0-then-close race that issue 0470 removed.
///
/// `ZenohRouter` carries all three fixes already, so this goes through the
/// shared fixture. Holding it in a `static` is still fine: `PR_SET_PDEATHSIG`,
/// not `Drop`, is what bounds the child when the parent is killed.
fn router_locator() -> Option<String> {
    static ROUTER: OnceLock<Mutex<Option<ZenohRouter>>> = OnceLock::new();
    let cell = ROUTER.get_or_init(|| {
        Mutex::new(match ZenohRouter::start_unique() {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!(
                    "[zenoh-cffi] could not start zenohd ({e:?}) — run \
                     `nros setup <board> --rmw zenoh`"
                );
                None
            }
        })
    });
    let guard = cell.lock().ok()?;
    guard.as_ref().map(|h| h.locator())
}

/// In-process pub→sub round-trip via the C vtable.
///
/// This was `#[ignore]`d as "architecturally broken" on 2026-05-11, citing
/// `zenoh_integration.rs::test_pubsub_loopback` as the same failure through
/// the Rust trait. That sibling has since been un-ignored and runs against a
/// router like this one; run again, this test passed 5 of 5. An ignore whose
/// stated reason no longer holds hides a live test, so it runs now.
///
/// Requires ROS's `rmw_zenohd` — nano-ros ships no router (RFC-0075). Start
/// one with `just zenohd`; the harness resolves it via `ros_zenohd_path`.
#[test]
fn cffi_pubsub_round_trip() {
    let locator =
        router_locator().expect("zenohd unavailable — run `nros setup <board> --rmw zenoh`");
    nros_rmw_zenoh::register().expect("register");

    let mut session = CffiSession::open(
        &locator,
        /* client */ 0,
        0,
        "l2_pubsub",
        /* root ns */ "",
    )
    .expect("open");
    // Match the existing nros-rmw-zenoh integration test shape: simple
    // short topic, `BEST_EFFORT` QoS (zenoh-pico's `RELIABLE` path
    // wants a full ROS-2-flavoured key prefix that the cffi shim
    // shouldn't have to know about), subscriber-first ordering with a
    // 1 s settle before the publisher comes up.
    let topic = TopicInfo::new("test/cffi_loopback", "Int32", "hash123");
    let qos = QoSProfile::BEST_EFFORT;

    let mut subscriber = session
        .create_subscription(&topic, qos)
        .expect("create_subscription");
    std::thread::sleep(Duration::from_secs(1));
    let publisher = session
        .create_publisher(&topic, qos)
        .expect("create_publisher");

    let payload: [u8; 8] = [
        0x00, 0x01, 0x00, 0x00, // CDR header LE
        0x39, 0x30, 0x00, 0x00, // i32 = 12345 LE
    ];

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut got: Option<usize> = None;
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        let _ = publisher.publish_raw(&payload);
        // zenoh-pico on POSIX runs its own RX thread inside
        // `_z_open`, but drive_io is the standard contract for
        // pull-based backends; call it so the test exercises that
        // path too.
        let _ = session.drive_io(50);
        std::thread::sleep(Duration::from_millis(100));
        match subscriber.take_serialized(&mut buf) {
            Ok(Some(n)) if n > 0 => {
                got = Some(n);
                break;
            }
            _ => continue,
        }
    }
    drop(publisher);
    drop(subscriber);
    drop(session);

    let n = got.expect("subscriber received no data within 10 s");
    assert_eq!(
        n,
        payload.len(),
        "got {n} bytes, expected {}",
        payload.len()
    );
    // First 4 bytes are the CDR-encapsulation header; bytes [4..8]
    // must match the i32 payload we serialised.
    assert_eq!(&buf[4..8], &payload[4..8]);
}

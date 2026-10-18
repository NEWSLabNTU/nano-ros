//! Issue 1940 — a C caller's per-subscription wake fires on a LIVE zenoh
//! arrival through the cffi route, and keeps firing after a take.
//!
//! This is the path `rclc_polling_subscription_set_wake_callback` takes: a
//! `c_waker` waker handed to the cffi subscription, forwarded through the
//! `subscription_set_wake` slot to `RustBackendAdapter`, and from there to the
//! zenoh shim's own `register_waker`. Before the fix the cffi handle had no
//! slot to forward to, so `register_waker` was a no-op and the counter stayed
//! at 0.
//!
//! Its own test binary, not a case in `cffi_smoke.rs`: the shipped shim holds
//! one session per process, and a second open in a process whose first session
//! has closed is refused under `cargo test`'s one-process threading.
//!
//! Requires ROS's `rmw_zenohd` (RFC-0075); an absent router is an unmet
//! precondition, never a pass.

#![cfg(feature = "platform-posix")]

use core::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use nros_rmw::{
    Publisher, QoSProfile, Session, Subscription, TopicInfo,
    c_waker::{CWakeState, make_waker},
};
use nros_rmw_cffi::CffiSession;
use nros_tests::fixtures::{ZenohRouter, or_unmet};

unsafe extern "C" fn count_wake(ctx: *mut core::ffi::c_void) {
    unsafe { (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst) };
}

#[test]
fn cffi_subscription_wake_fires_on_a_live_arrival() {
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::unmet!("{why}");
    }
    let router = or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();
    nros_rmw_zenoh::register().expect("register");
    let mut session = CffiSession::open(&locator, /* client */ 0, 0, "l2_wake", "").expect("open");
    let topic = TopicInfo::new("test/cffi_wake", "Int32", "hash1940");
    let qos = QoSProfile::BEST_EFFORT;
    let mut subscriber = session
        .create_subscription(&topic, qos)
        .expect("create_subscription");

    // `rclc_wake_state_t` storage: stable for the subscription's life.
    let fired: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
    let state: &'static CWakeState = Box::leak(Box::new(CWakeState {
        fn_ptr: Some(count_wake),
        ctx: fired as *const AtomicU32 as *mut core::ffi::c_void,
    }));
    let waker = unsafe { make_waker(state) };
    assert!(
        subscriber.register_waker(&waker),
        "zenoh can wake a subscription, so the cffi route must report it can"
    );

    std::thread::sleep(Duration::from_secs(1));
    let publisher = session
        .create_publisher(&topic, qos)
        .expect("create_publisher");
    let payload: [u8; 8] = [0x00, 0x01, 0x00, 0x00, 0x94, 0x07, 0x00, 0x00];
    let mut buf = [0u8; 64];

    // Publish until the CALLBACK fires — no take before it, so a pass cannot
    // come from polling.
    let wait_for = |target: u32| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && fired.load(Ordering::SeqCst) < target {
            let _ = publisher.publish_raw(&payload);
            std::thread::sleep(Duration::from_millis(100));
        }
        fired.load(Ordering::SeqCst) >= target
    };
    assert!(wait_for(1), "no wake callback within 10 s of publishing");

    // Drain, which re-arms the one-shot shim waker; the next arrival must fire
    // again.
    while let Ok(Some(_)) = subscriber.take_serialized(&mut buf) {}
    let before = fired.load(Ordering::SeqCst);
    assert!(wait_for(before + 1), "the wake did not survive the take");

    drop(publisher);
    drop(subscriber);
    drop(session);
}

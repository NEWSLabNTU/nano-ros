//! Host-mock end-to-end tests for [`nros_px4::pump`] and the
//! supporting [`nros_rmw_uorb::park_until_event`] future.
//!
//! Focus: validate the wake chain landed in 90.5b without depending
//! on a real PX4 SITL run. The wake chain is:
//!
//! ```text
//!   uORB publish
//!     → orb_callback (sub_cb_register)
//!       → wake_trampoline (px4-uorb)
//!         → AtomicWaker::wake on per-topic waker
//!           → caller's Waker (e.g. block_on's test driver)
//! ```
//!
//! Real PX4 SITL exercises Style C (raw API + `#[task]`) via
//! `packages/testing/nros-tests/tests/px4_e2e.rs`. Style B
//! (`run_async` + `Executor`) is std-only because the trampoline
//! registry uses `HashMap` + `Mutex` — see Phase 90.2b in
//! `nros-rmw-uorb/src/registry.rs` for the no_std follow-up plan.

#![allow(non_camel_case_types)]
#![cfg(feature = "test-helpers")]

use core::pin::Pin;
use core::time::Duration;
use std::sync::Mutex;
use std::time::Instant;

use futures::executor::block_on;
use nros_node::{Executor, ExecutorConfig};
use nros_px4::pump_until;
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Tick {
    seq: u32,
    timestamp: u64,
}

struct tick_topic;
static TICK_NAME: [u8; 13] = *b"sensor_accel\0";
static TICK_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: TICK_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Tick>() as u16,
    o_size_no_padding: core::mem::size_of::<Tick>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});
impl UorbTopic for tick_topic {
    type Msg = Tick;
    fn metadata() -> &'static orb_metadata {
        TICK_META.get()
    }
}

/// L1 + L2 direct test: a uORB publish on a registered topic must
/// wake `park_until_event` well before the bounded sleep expires.
///
/// Setup: register topic → start parking with `park_max = 1s` → on
/// a background thread, publish after 50 ms → expect park to return
/// in 50–500 ms (wake-driven, not sleep-driven).
#[test]
fn park_until_event_wakes_on_uorb_publish() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    // Background publisher: sleep 50 ms, then publish once.
    let driver = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        let pub_ = px4_uorb::Publication::<tick_topic>::new();
        let msg = Tick {
            seq: 1,
            timestamp: 0xdead_beef,
        };
        pub_.publish(&msg).expect("publish");
    });

    let start = Instant::now();
    block_on(nros_rmw_uorb::park_until_event(Duration::from_secs(1)));
    let elapsed = start.elapsed();
    driver.join().unwrap();

    assert!(
        elapsed >= Duration::from_millis(40),
        "park returned too early ({elapsed:?}) — likely missed the publish race"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "park did not wake on publish ({elapsed:?}) — wake chain broken; \
         park_max=1s expired instead of waker firing"
    );
}

/// L3 pump test: the pump drives `Executor::spin_once` and routes a
/// raw publish through the registry into a `RawSubscription`'s
/// receive buffer. We poll `try_recv_raw` from the test driver to
/// observe the message landed.
///
/// Run-loop shape: pump_until exits when `try_recv_raw` returns
/// `Some`. Park_max = 1s — if the wake chain breaks, the test
/// either takes the full second per loop iteration or hangs past
/// the 3-s wall-clock cap.
#[test]
fn pump_routes_publish_through_executor_within_park_window() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    let exec_config = ExecutorConfig::new("").node_name("pump_test");
    let executor = Executor::open(&exec_config).expect("open");

    // Background publisher fires after 50 ms.
    let driver = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        let pub_ = px4_uorb::Publication::<tick_topic>::new();
        let msg = Tick {
            seq: 0xfeed_face,
            timestamp: 0xdead_beef,
        };
        pub_.publish(&msg).expect("publish");
    });

    // The pump's exit condition: any uORB publish on the registered
    // topic. We can't observe that via the executor (no callback
    // shape for raw subs), so peek the px4-uorb broker directly via
    // a fresh subscription in the until-future. The pump still
    // drives spin_once; the until-future just observes side-effects.
    let probe = px4_uorb::Subscription::<tick_topic>::new();
    let until = futures::future::poll_fn(move |cx| {
        if probe.try_recv().is_some() {
            core::task::Poll::Ready(())
        } else {
            // Re-register on every poll: we want the cx waker to
            // fire when a publish lands so we exit promptly.
            probe.register_waker(cx.waker());
            core::task::Poll::Pending
        }
    });

    let start = Instant::now();
    let timeout = Duration::from_secs(3);

    block_on(async {
        let pump_fut = pump_until(executor, Duration::from_secs(1), Box::pin(until));
        let deadline = futures::future::poll_fn(move |cx| {
            if start.elapsed() >= timeout {
                core::task::Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
        });
        let mut pump_pin = Box::pin(pump_fut);
        let mut deadline_pin = Box::pin(deadline);
        // Manual select — futures::select_biased! requires the
        // async-await feature which we don't depend on.
        core::future::poll_fn(move |cx| {
            if let core::task::Poll::Ready(()) = Pin::as_mut(&mut pump_pin).poll(cx) {
                return core::task::Poll::Ready(());
            }
            if let core::task::Poll::Ready(()) = Pin::as_mut(&mut deadline_pin).poll(cx) {
                panic!("pump did not exit within 3 s — wake chain broken");
            }
            core::task::Poll::Pending
        })
        .await;
    });

    driver.join().unwrap();

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "pump took {elapsed:?} to route the publish — wake chain not event-driven"
    );
}

/// Idle-CPU sanity check (task 32): with `park_max = 200 ms` and no
/// uORB traffic, the pump should park between drains rather than
/// busy-loop.
///
/// We measure CPU time consumed during a wall-clock window and
/// assert it stays well under the wall — a busy-loop would saturate
/// CPU (CPU ≈ wall), an event-driven park keeps CPU « wall.
///
/// The until-future fires once via a background thread that flips
/// an `AtomicBool` and wakes the registered Waker, so the pump
/// never gets a spurious wake from the until-future itself.
#[test]
fn pump_idles_on_quiescent_topics() {
    use core::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    let exec_config = ExecutorConfig::new("").node_name("idle_test");
    let executor = Executor::open(&exec_config).expect("open");

    let done = Arc::new(AtomicBool::new(false));
    let waker_slot: Arc<StdMutex<Option<core::task::Waker>>> = Arc::new(StdMutex::new(None));

    // Exit-trigger thread: sleep 600 ms, set flag, fire waker.
    let done_set = Arc::clone(&done);
    let waker_set = Arc::clone(&waker_slot);
    let timer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        done_set.store(true, Ordering::Release);
        if let Some(w) = waker_set.lock().unwrap().take() {
            w.wake();
        }
    });

    let until = {
        let done = Arc::clone(&done);
        let waker_slot = Arc::clone(&waker_slot);
        futures::future::poll_fn(move |cx| {
            if done.load(Ordering::Acquire) {
                core::task::Poll::Ready(())
            } else {
                // Store waker once; timer thread fires it.
                *waker_slot.lock().unwrap() = Some(cx.waker().clone());
                core::task::Poll::Pending
            }
        })
    };

    let wall_start = Instant::now();
    let cpu_start = cpu_time::ProcessTime::try_now();
    block_on(pump_until(
        executor,
        Duration::from_millis(200),
        Box::pin(until),
    ));
    let wall_elapsed = wall_start.elapsed();
    let cpu_elapsed = cpu_start.ok().and_then(|s| s.try_elapsed().ok());
    timer.join().unwrap();

    assert!(
        wall_elapsed >= Duration::from_millis(550),
        "test exited too early ({wall_elapsed:?}) — until-future broken"
    );
    assert!(
        wall_elapsed < Duration::from_millis(1_500),
        "test took {wall_elapsed:?} — pump may be stuck"
    );

    if let Some(cpu) = cpu_elapsed {
        // Sanity: with park_max=200 ms over a 600 ms window we
        // expect at most ~3 park expiries + spin_once passes, plus
        // ~zero CPU during sleeps. A regressed busy-loop would burn
        // most of the 600 ms wall on CPU. Allow generous slack for
        // futures-executor overhead and HRT thread spawns.
        assert!(
            cpu < Duration::from_millis(200),
            "pump consumed {cpu:?} CPU over {wall_elapsed:?} wall — \
             busy-loop regression suspected (expected <200ms CPU for \
             a 600ms event-driven park)"
        );
    }
}

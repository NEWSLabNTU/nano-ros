//! `park_until_event(max)` — async park future used by
//! [`nros_px4::run_async`] to suspend the executor task between
//! drain passes.
//!
//! Races a bounded [`px4_workqueue::Sleep`] against the per-topic
//! `AtomicWaker`s held by every registered subscription. Any uORB
//! publish on a registered topic fires its waker, which calls
//! `ScheduleNow` on the executor's hosting `WorkItem`, which polls
//! the future, which returns `Ready` so the executor can drain.
//!
//! `Sleep` is the safety net: nros timers and any wake source not
//! routed through the registry (currently `GuardCondition`) only get
//! attention at the next sleep expiry. Pick `max` to bound that
//! latency. 50 ms is a good default for apps with sub-100 ms timers.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

use px4_workqueue::Sleep;
use px4_workqueue::sleep as wq_sleep;

use crate::registry::register_wake_on_all;

/// Park the calling async task until either:
/// - any registered uORB subscription receives a publish, OR
/// - `max` elapses.
///
/// On every `Pending` return, the caller's `Waker` is registered on
/// every active `TopicHandle`'s `AtomicWaker` (single-slot — only
/// safe with a single parking awaiter).
pub fn park_until_event(max: Duration) -> Park {
    Park {
        sleep: wq_sleep(max),
        polled: false,
        _pin: PhantomPinned,
    }
}

/// Future returned by [`park_until_event`]. `!Unpin` — the inner
/// `Sleep` registers the HRT callback against its own address.
///
/// Two-state machine:
/// - **first poll**: arm Sleep, register caller's waker on every
///   subscription's `AtomicWaker`, return `Pending`.
/// - **subsequent poll**: any re-poll means *something* woke us
///   (Sleep expiry or any uORB callback). Return `Ready` so the
///   caller drains.
///
/// The "second-poll = wake fired" rule trades precision for
/// simplicity. Spurious wakes (the executor re-polling without a
/// real wake source) cause an extra `spin_once` pass, which is
/// harmless. The alternative — holding a separate `AtomicBool`
/// flipped by an interposed waker — adds complexity without
/// changing the steady-state CPU profile.
pub struct Park {
    sleep: Sleep,
    polled: bool,
    _pin: PhantomPinned,
}

impl Future for Park {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: we never move `self` or `self.sleep`. `Park` is
        // `!Unpin`, and `Pin::new_unchecked` projects through the
        // outer pin to the inner field whose address is stable.
        let this = unsafe { self.get_unchecked_mut() };
        let sleep_pin = unsafe { Pin::new_unchecked(&mut this.sleep) };

        if let Poll::Ready(()) = sleep_pin.poll(cx) {
            return Poll::Ready(());
        }

        if this.polled {
            // We were re-polled while the Sleep was still Pending —
            // something else woke our task (a uORB callback, most
            // likely). Return Ready so the caller can drain.
            return Poll::Ready(());
        }

        this.polled = true;

        // Sleep returned Pending and registered our waker on its own
        // AtomicWaker. Also register on every uORB sub. The waker is
        // a clone-counted handle to the same destination
        // (WorkItemCell::poll_once via ScheduleNow), so any of them
        // firing schedules a re-poll.
        register_wake_on_all(cx.waker());

        Poll::Pending
    }
}

// Host-mock unit tests for `park_until_event` are wired in
// `tests/park_e2e.rs` so they can pull `px4-workqueue/std` mock and a
// futures executor as dev-dependencies without leaking those deps into
// the production build.

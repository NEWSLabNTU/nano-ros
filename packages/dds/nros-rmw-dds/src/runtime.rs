//! Phase 71.3 — `NrosPlatformRuntime<P>` adapter.
//!
//! Thin adapter that implements dust-dds's [`DdsRuntime`] trait on top of
//! `nros-platform-api`'s capability traits (`PlatformClock`, `PlatformSleep`,
//! plus an alloc-backed cooperative task queue). No background threads; the
//! task queue is drained by `nros_node::Executor::spin_once()` through the
//! arena hook added in Phase 71.4 (pending).
//!
//! # Platform choice
//!
//! The runtime is generic over a `P: PlatformClock + PlatformSleep`. The
//! canonical instantiation is `NrosPlatformRuntime<nros_platform::ConcretePlatform>`,
//! resolved from whichever `platform-*` Cargo feature the consumer enabled.
//! Tests / host fixtures may instantiate with explicit platform ZSTs.
//!
//! # Status
//!
//! | Piece             | Status  | Notes                                     |
//! |-------------------|---------|-------------------------------------------|
//! | `Clock`           | Working | Dispatches `<P as PlatformClock>::clock_ms()` |
//! | `Timer`           | Working | Deadline-polled future (see `NrosSleep`)    |
//! | `Spawner`         | Working | `Arc<Mutex<VecDeque<_>>>` task queue; `drain_tasks()` polls once per call |
//! | `DdsRuntime` impl | Working | Phase 71.3 scaffolding                    |
//! | Wired into `Rmw::open` for non-POSIX | **Pending 71.4 + 71.2** | Needs the arena hook + non-blocking transport |
//! | Sync `block_on`   | **Pending 71.1** | Dust-dds sync API still imports `std_runtime::executor::block_on` directly; the fork patch reintroduces `DdsRuntime::block_on` |

#![cfg(feature = "alloc")]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::time::Duration;

use dust_dds::{
    infrastructure::time::Time,
    runtime::{Clock, DdsRuntime, Spawner, Timer},
};
use nros_platform::{PlatformClock, PlatformSleep};

// ---------------------------------------------------------------------------
// Lock primitive
// ---------------------------------------------------------------------------

// When `std` is active, use `std::sync::Mutex`; otherwise use `spin::Mutex`.
// Both satisfy the `Send + Sync` bounds that dust-dds's handle types
// require. Keeping the selection here means the rest of the file never
// has to `#[cfg]`-gate.

#[cfg(feature = "std")]
type Mutex<T> = std::sync::Mutex<T>;
#[cfg(feature = "std")]
fn mutex_new<T>(v: T) -> Mutex<T> {
    std::sync::Mutex::new(v)
}
#[cfg(feature = "std")]
fn mutex_lock<T, R>(m: &Mutex<T>, f: impl FnOnce(&mut T) -> R) -> R {
    let mut g = m.lock().expect("poisoned");
    f(&mut *g)
}

#[cfg(not(feature = "std"))]
type Mutex<T> = spin::Mutex<T>;
#[cfg(not(feature = "std"))]
fn mutex_new<T>(v: T) -> Mutex<T> {
    spin::Mutex::new(v)
}
#[cfg(not(feature = "std"))]
fn mutex_lock<T, R>(m: &Mutex<T>, f: impl FnOnce(&mut T) -> R) -> R {
    let mut g = m.lock();
    f(&mut *g)
}

// ---------------------------------------------------------------------------
// Clock handle
// ---------------------------------------------------------------------------

/// dust-dds `Clock` backed by `<P as PlatformClock>::clock_ms()`.
///
/// The phantom type uses `fn() -> P` rather than `P` directly so the
/// resulting struct is unconditionally `Send + Sync` regardless of whether
/// the platform ZST itself is `Send + Sync` (it always is — but the
/// `fn`-form `PhantomData` makes that independent of trait bounds on `P`).
pub struct NrosClock<P>(PhantomData<fn() -> P>);

impl<P> Default for NrosClock<P> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P> Clone for NrosClock<P> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<P> Clock for NrosClock<P>
where
    P: PlatformClock + 'static,
{
    fn now(&self) -> Time {
        let ms = <P as PlatformClock>::clock_ms();
        Time::new((ms / 1000) as i32, ((ms % 1000) as u32) * 1_000_000)
    }
}

// ---------------------------------------------------------------------------
// Timer handle
// ---------------------------------------------------------------------------

/// A deadline-based sleep future that polls the platform clock every time
/// it is polled. Ready once `clock_ms() >= deadline_ms`.
///
/// When not yet ready the future calls `wake_by_ref()` on its own waker so
/// the cooperative executor immediately re-schedules it on the next spin;
/// this produces busy-polling but stays O(N) per active delay which is
/// acceptable for the small number of reliability / heartbeat timers RTPS
/// spins up in a typical embedded deployment. A later iteration can track
/// deadlines in a heap and only re-wake the nearest one, similar to
/// `std_runtime::timer::TimerHeap`.
pub struct NrosSleep<P> {
    deadline_ms: u64,
    _p: PhantomData<fn() -> P>,
}

impl<P> Future for NrosSleep<P>
where
    P: PlatformClock + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if <P as PlatformClock>::clock_ms() >= self.deadline_ms {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

// Safety: `NrosSleep<P>` contains only a `u64` and `PhantomData<fn() -> P>`,
// both of which are `Send`.
unsafe impl<P> Send for NrosSleep<P> {}

pub struct NrosTimer<P>(PhantomData<fn() -> P>);

impl<P> Default for NrosTimer<P> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P> Clone for NrosTimer<P> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<P> Timer for NrosTimer<P>
where
    P: PlatformClock + 'static,
{
    fn delay(&mut self, duration: Duration) -> impl Future<Output = ()> + Send {
        let now_ms = <P as PlatformClock>::clock_ms();
        let deadline_ms = now_ms.saturating_add(duration.as_millis() as u64);
        NrosSleep::<P> {
            deadline_ms,
            _p: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// Spawner handle
// ---------------------------------------------------------------------------

/// Type-erased future the spawner owns.
type BoxedTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Task queue shared between the spawner handle and the cooperative
/// executor that drains it.
///
/// Every `spawn()` push is O(1); `drain_tasks()` pops each pending task,
/// polls it once with a no-op waker, and re-pushes it onto a fresh queue
/// if it returned `Pending`. The no-op waker is fine for the deadline-poll
/// style used by `NrosSleep` because that future calls `wake_by_ref()`
/// itself to request re-polling — a more general executor would thread a
/// per-task `AtomicBool` waker in.
#[derive(Clone)]
pub struct NrosSpawner {
    queue: Arc<Mutex<VecDeque<BoxedTask>>>,
}

impl NrosSpawner {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(mutex_new(VecDeque::new())),
        }
    }

    /// Drain the task queue once: pop every pending task, poll it, and
    /// push it back onto a second queue if it didn't complete. Intended
    /// to be called from the executor arena hook (Phase 71.4).
    pub fn drain_tasks(&self) {
        let drained: VecDeque<BoxedTask> =
            mutex_lock(&self.queue, |q| core::mem::take(q));
        let mut survivors: VecDeque<BoxedTask> = VecDeque::with_capacity(drained.len());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        for mut task in drained {
            if task.as_mut().poll(&mut cx).is_pending() {
                survivors.push_back(task);
            }
        }
        // Re-queue survivors *after* polling so tasks that were newly
        // spawned *during* a poll (via the Spawner handle clone) don't
        // get shadowed.
        mutex_lock(&self.queue, |q| {
            for t in survivors {
                q.push_back(t);
            }
        });
    }

    pub fn is_empty(&self) -> bool {
        mutex_lock(&self.queue, |q| q.is_empty())
    }
}

impl Default for NrosSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spawner for NrosSpawner {
    fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
        let boxed: BoxedTask = Box::pin(f);
        mutex_lock(&self.queue, |q| q.push_back(boxed));
    }
}

// ---------------------------------------------------------------------------
// No-op waker — used by `drain_tasks` pending Phase 71.4's per-task waker
// ---------------------------------------------------------------------------

fn noop_waker() -> Waker {
    use core::task::{RawWaker, RawWakerVTable};
    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(core::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    // Safety: the vtable's clone/wake/drop functions ignore the data ptr
    // and never dereference it; a null pointer is valid input.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// dust-dds `DdsRuntime` backed by `nros-platform-api` primitives.
///
/// Generic over `P` so test fixtures can pin a specific platform ZST;
/// the shipped configuration uses `P = nros_platform::ConcretePlatform`
/// resolved from the `platform-*` Cargo feature.
///
/// `Clone` is intentionally cheap — only the spawner's `Arc` is
/// cloned. Cloning the runtime gives every clone access to the same
/// task queue, which is the right semantics for both
/// `DomainParticipantFactoryAsync::new(runtime, ...)` (consumes one
/// clone) and `runtime.block_on(...)` (uses another clone to drive
/// the same task queue from a different call site).
pub struct NrosPlatformRuntime<P> {
    spawner: NrosSpawner,
    _p: PhantomData<fn() -> P>,
}

impl<P> Clone for NrosPlatformRuntime<P> {
    fn clone(&self) -> Self {
        Self {
            spawner: self.spawner.clone(),
            _p: PhantomData,
        }
    }
}

impl<P> NrosPlatformRuntime<P> {
    pub fn new() -> Self {
        Self {
            spawner: NrosSpawner::new(),
            _p: PhantomData,
        }
    }

    /// Drain the spawner's task queue once. Intended for Phase 71.4's
    /// arena hook to call from `Executor::spin_once()`.
    pub fn drive(&self) {
        self.spawner.drain_tasks();
    }

    pub fn spawner_handle(&self) -> NrosSpawner {
        self.spawner.clone()
    }
}

impl<P> NrosPlatformRuntime<P>
where
    P: PlatformClock + PlatformSleep + 'static,
{
    /// Phase 71.1 — block on a future until it resolves, driving the
    /// spawner's background task queue on each iteration.
    ///
    /// Unlike `dust_dds::std_runtime::executor::block_on`, this does **not**
    /// park the OS thread; it yields back to the platform via
    /// `<P as PlatformSleep>::sleep_ms(1)` whenever all tasks returned
    /// `Pending`. That keeps the CPU cool on POSIX and lets cooperative
    /// RTOSes (Zephyr, FreeRTOS) give time to other threads. Same
    /// semantics as `zpico::Context::zpico_get` on the ZPICO side —
    /// drive the runtime cooperatively, never block on a condvar.
    ///
    /// Used by `nros-rmw-dds` to wrap dust-dds's async API (`dds_async`)
    /// on every non-POSIX platform. On `std + platform-posix` we fall
    /// through to `dust_dds::std_runtime::executor::block_on` for
    /// compatibility with the stock transport's OS-thread model.
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        use core::pin::pin;

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut f = pin!(future);
        loop {
            // Poll the caller's future first; if it's already ready we
            // don't need to drive any background work.
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            // Drive background tasks (RTPS receive loops, reliability
            // timers, etc.) once per iteration.
            self.spawner.drain_tasks();
            // Yield to the platform scheduler so we don't starve
            // co-resident threads (POSIX) or ISR handlers (RTOS).
            <P as PlatformSleep>::sleep_ms(1);
        }
    }
}

impl<P> Default for NrosPlatformRuntime<P> {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: `NrosPlatformRuntime<P>`'s only non-`PhantomData` field is
// `spawner: NrosSpawner`, which is `Send` (it contains
// `Arc<Mutex<VecDeque<Pin<Box<dyn Future<..> + Send>>>>>`, all `Send`).
unsafe impl<P: 'static> Send for NrosPlatformRuntime<P> {}

impl<P> DdsRuntime for NrosPlatformRuntime<P>
where
    P: PlatformClock + PlatformSleep + 'static,
{
    type ClockHandle = NrosClock<P>;
    type TimerHandle = NrosTimer<P>;
    type SpawnerHandle = NrosSpawner;

    fn timer(&self) -> Self::TimerHandle {
        NrosTimer::<P>::default()
    }

    fn clock(&self) -> Self::ClockHandle {
        NrosClock::<P>::default()
    }

    fn spawner(&self) -> Self::SpawnerHandle {
        self.spawner.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use nros_platform::ConcretePlatform;

    #[test]
    fn clock_returns_monotonic_time() {
        let clock: NrosClock<ConcretePlatform> = NrosClock::default();
        let a = clock.now();
        std::thread::sleep(core::time::Duration::from_millis(5));
        let b = clock.now();
        // b must be >= a
        assert!(b.sec() > a.sec() || (b.sec() == a.sec() && b.nanosec() >= a.nanosec()));
    }

    #[test]
    fn spawner_runs_ready_future() {
        let s = NrosSpawner::new();
        use alloc::sync::Arc;
        use core::sync::atomic::{AtomicBool, Ordering};
        let flag = Arc::new(AtomicBool::new(false));
        let flag_c = flag.clone();
        s.spawn(async move {
            flag_c.store(true, Ordering::SeqCst);
        });
        assert!(!flag.load(Ordering::SeqCst));
        s.drain_tasks();
        assert!(flag.load(Ordering::SeqCst));
        assert!(s.is_empty());
    }

    #[test]
    fn block_on_resolves_ready_future() {
        let rt: NrosPlatformRuntime<ConcretePlatform> = NrosPlatformRuntime::new();
        let v = rt.block_on(async { 42u32 });
        assert_eq!(v, 42);
    }

    #[test]
    fn block_on_drives_spawned_side_task() {
        use alloc::sync::Arc;
        use core::sync::atomic::{AtomicU32, Ordering};
        let rt: NrosPlatformRuntime<ConcretePlatform> = NrosPlatformRuntime::new();
        let counter = Arc::new(AtomicU32::new(0));
        // Background task that pings twice then completes.
        {
            let c = counter.clone();
            let s = rt.spawner();
            s.spawn(async move {
                for _ in 0..2 {
                    c.fetch_add(1, Ordering::SeqCst);
                    // yield once per iteration
                    struct YieldOnce(bool);
                    impl Future for YieldOnce {
                        type Output = ();
                        fn poll(
                            mut self: Pin<&mut Self>,
                            cx: &mut Context<'_>,
                        ) -> Poll<()> {
                            if self.0 {
                                Poll::Ready(())
                            } else {
                                self.0 = true;
                                cx.waker().wake_by_ref();
                                Poll::Pending
                            }
                        }
                    }
                    YieldOnce(false).await;
                }
            });
        }
        // Foreground future that completes after the background task has
        // incremented the counter at least twice.
        let counter_fg = counter.clone();
        rt.block_on(async move {
            while counter_fg.load(Ordering::SeqCst) < 2 {
                struct YieldOnce(bool);
                impl Future for YieldOnce {
                    type Output = ();
                    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                        if self.0 {
                            Poll::Ready(())
                        } else {
                            self.0 = true;
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    }
                }
                YieldOnce(false).await;
            }
        });
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn spawner_reschedules_pending_future() {
        let s = NrosSpawner::new();
        use core::sync::atomic::{AtomicU32, Ordering};
        use alloc::sync::Arc;
        let polls = Arc::new(AtomicU32::new(0));
        let polls_c = polls.clone();
        s.spawn(async move {
            // Use a manual Pending-then-Ready future.
            struct TwoPhase {
                polls: Arc<AtomicU32>,
            }
            impl Future for TwoPhase {
                type Output = ();
                fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                    let n = self.polls.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                }
            }
            TwoPhase { polls: polls_c }.await;
        });
        s.drain_tasks();
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(!s.is_empty());
        s.drain_tasks();
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(s.is_empty());
    }
}

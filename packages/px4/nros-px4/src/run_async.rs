//! `run_async(config, user_fn)` — proper-waker variant of [`crate::run`].
//!
//! Phase 90.5b: replaces the 10 ms busy `spin_once` loop with a
//! `px4-workqueue` [`WorkItemCell`] driving an async pump. Between
//! drain passes the pump parks via
//! [`nros_rmw_uorb::park_until_event`], which races a bounded
//! [`px4_workqueue::Sleep`] against the per-topic `AtomicWaker`s
//! registered on every active uORB subscription. Any uORB publish
//! fires its waker → `ScheduleNow` → next poll → next drain.
//!
//! Acceptance:
//! - `run_async` returns control to the WorkQueue thread between
//!   drains (no busy-polling on quiescent topics).
//! - uORB-driven topics retain zero-latency wake (event-driven via
//!   the uORB callback chain, not the bounded sleep).
//! - nros timers stay accurate to within `Config::park_max`.

use core::future::Future;
use core::pin::Pin;
use core::time::Duration;

use nros_node::{Executor, ExecutorConfig};
use px4_workqueue::{WorkItemCell, WqConfig, wq_configurations, yield_now};

use crate::Config;

/// Concrete future type erased through `Pin<Box<dyn Future>>` so the
/// hosting [`WorkItemCell`] static can name it without leaking the
/// user closure's anonymous future type.
type NrosFut = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// Single static `WorkItemCell` owning the executor pump task.
///
/// `WorkItemCell::spawn` requires `&'static self`; using a single
/// static slot means each PX4 module hosts exactly one
/// `nros_px4::run_async` (matching the one-Executor-per-WQ
/// architectural rule). A second call returns
/// `Err(SpawnError::Busy)` — converted here into a panic to surface
/// the misuse early.
static CELL: WorkItemCell<NrosFut> = WorkItemCell::new();

/// Run a nano-ros executor under PX4 with a proper waker chain.
/// Never returns.
///
/// Differences from [`crate::run`]:
/// - Spawns the executor on a `px4-workqueue` [`WorkItemCell`]
///   instead of polling on the calling thread.
/// - Parks via [`nros_rmw_uorb::park_until_event`] between drains.
/// - Wakes on uORB publish (zero latency) or `Config::park_max`
///   expiry — whichever comes first.
pub fn run_async<F, E>(config: Config<'static>, user_fn: F) -> !
where
    F: FnOnce(&mut Executor) -> Result<(), E>,
    E: core::fmt::Debug,
{
    let exec_config = ExecutorConfig::new("")
        .node_name(config.node_name)
        .namespace(config.namespace);

    let mut executor =
        Executor::open(&exec_config).expect("nros-px4: failed to open uORB-backed executor");

    if let Err(e) = user_fn(&mut executor) {
        // Drop the error; in real PX4 this would be `px4_log::err!`.
        let _ = e;
    }

    let park_max = config.park_max;
    let task: NrosFut = Box::pin(pump(executor, park_max));

    // SAFETY: `&'static CELL` — the static lives for the program
    // lifetime, satisfying `WorkItemCell::spawn`'s `&'static self`
    // contract. `wq` is also `&'static` — looked up by name from
    // `wq_configurations`.
    let wq: &'static WqConfig = wq_for(config.wq_name);
    let task_name = c"nros_px4";
    CELL.spawn(task, wq, task_name).forget();

    // The WorkItem now owns the executor. PX4 modules typically
    // return from `*_main` after spawning their work item; the
    // module's `start` shell command prints a result line and
    // exits. Park here so the caller's `run_async` signature
    // stays `-> !` matching `run`.
    loop {
        // Block the calling thread (the shell-command thread, not
        // the WQ thread). On real PX4 this is a sleep; on host
        // mock the test runner shuts down via SIGTERM.
        std::thread::park();
    }
}

/// The async pump. Drains the executor, then parks until any wake
/// source fires.
///
/// Exposed so integration tests (and users with custom
/// orchestration needs) can drive the same pump on a different
/// `WorkItemCell` or against a host-mock dispatcher.
pub async fn pump(mut executor: Executor, park_max: Duration) {
    loop {
        let result = executor.spin_once(Duration::ZERO);
        if !result.any_work() {
            // Nothing to do. Park until any uORB sub fires or
            // park_max expires.
            nros_rmw_uorb::park_until_event(park_max).await;
        } else {
            // Dispatched at least one callback. Yield once so any
            // other WorkItem on the same WQ that became ready in
            // the meantime gets a turn before we drain again.
            yield_now().await;
        }
    }
}

/// Test-only variant of [`pump`] that returns once `until` fires.
/// Used by integration tests to exit the pump cleanly after
/// asserting some condition.
#[cfg(any(test, feature = "test-helpers"))]
pub async fn pump_until<U>(mut executor: Executor, park_max: Duration, until: U)
where
    U: Future<Output = ()> + Unpin,
{
    use core::pin::Pin;
    use core::task::Poll;

    let mut until = until;
    loop {
        // Check the exit condition first — race the user's signal
        // against any pump activity.
        let exit_now = core::future::poll_fn(|cx| {
            if let Poll::Ready(()) = Pin::new(&mut until).poll(cx) {
                Poll::Ready(true)
            } else {
                Poll::Ready(false)
            }
        })
        .await;
        if exit_now {
            return;
        }

        let result = executor.spin_once(Duration::ZERO);
        if !result.any_work() {
            // Park properly — same shape as `pump`. Holds one Sleep
            // for the duration of the park, not one per poll.
            //
            // Race the park against `until` so an exit signal during
            // a long park_max wakes us promptly.
            let park = nros_rmw_uorb::park_until_event(park_max);
            let mut park = Box::pin(park);
            let woken = core::future::poll_fn(|cx| {
                if let Poll::Ready(()) = Pin::new(&mut until).poll(cx) {
                    return Poll::Ready(true);
                }
                if let Poll::Ready(()) = park.as_mut().poll(cx) {
                    return Poll::Ready(false);
                }
                Poll::Pending
            })
            .await;
            if woken {
                return;
            }
        } else {
            yield_now().await;
        }
    }
}

/// Map `Config::wq_name` to a `&'static WqConfig` from
/// `px4_workqueue::wq_configurations`. Panics on unknown names —
/// PX4 only ships a fixed set of WorkQueues.
fn wq_for(name: &str) -> &'static WqConfig {
    match name {
        "lp_default" => &wq_configurations::lp_default,
        "rate_ctrl" => &wq_configurations::rate_ctrl,
        "hp_default" => &wq_configurations::hp_default,
        "uavcan" => &wq_configurations::uavcan,
        "nav_and_controllers" => &wq_configurations::nav_and_controllers,
        "INS0" => &wq_configurations::INS0,
        "INS1" => &wq_configurations::INS1,
        "INS2" => &wq_configurations::INS2,
        "INS3" => &wq_configurations::INS3,
        other => panic!(
            "nros-px4: unknown WQ name {:?}; pick one of \
             lp_default, rate_ctrl, hp_default, uavcan, \
             navigation_and_controllers, INS0..3",
            other
        ),
    }
}

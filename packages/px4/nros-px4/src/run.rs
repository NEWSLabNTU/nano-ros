//! `run(config, user_fn)` — board-style entry point.
//!
//! Opens an [`nros_node::Executor`] backed by [`nros_rmw_uorb::UorbSession`],
//! invokes the user closure to register nodes/publishers/subscribers, then
//! parks in a spin loop.
//!
//! Phase 90.5 baseline: simple synchronous spin loop. Phase 90.5b will
//! migrate to a px4-workqueue task whose waker is signalled by uORB
//! subscription callbacks via `ScheduleNow()` — eliminating the polling
//! cost.

use core::time::Duration;

use nros_node::{Executor, ExecutorConfig};

use crate::Config;

/// Run a nano-ros executor under PX4. Never returns.
pub fn run<F, E>(config: Config<'_>, user_fn: F) -> !
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
        // PX4 module panic → px4_log::err! once we wire the panic handler.
        // For now drop the error so the caller can choose to abort.
        let _ = e;
    }

    loop {
        let _ = executor.spin_once(Duration::from_millis(10));
        // Phase 90.5b: replace the busy-loop with a WorkItem-park so other
        // PX4 tasks can preempt cleanly. For now spin_once internally yields
        // via the platform's PlatformYield impl.
    }
}

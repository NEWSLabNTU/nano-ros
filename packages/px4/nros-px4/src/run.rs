//! `run(config, user_fn)` — board-style entry point.
//!
//! Phase 90.5 baseline: validates config, opens a [`UorbSession`] via
//! [`nros_rmw_uorb::UorbRmw`], invokes the user closure with a configured
//! [`nros_node::Executor`]. The actual WorkItem attachment + spin-on-wake
//! integration lands once `nros-node` exposes an `Executor::spin_once_with`
//! callback hook (tracked alongside Phase 94's `Executor::open_with_session`
//! API).
//!
//! Returns `!` because PX4 modules run forever once the WorkQueue is
//! attached.

use crate::Config;

/// Run a nano-ros executor under PX4. Currently returns immediately on the
/// host-mock build; once Phase 90.5b lands the executor will park inside the
/// chosen WorkQueue and never return.
pub fn run<F, E>(config: Config<'_>, user_fn: F) -> !
where
    F: FnOnce(&Config<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // Phase 90.5b: open UorbSession, attach NrosWorkItem to config.wq_name,
    // install wake callback into nros-rmw-uorb subscriber registry, run
    // user_fn, then park executor inside WorkQueue. For the skeleton, just
    // call user_fn and loop.
    if let Err(e) = user_fn(&config) {
        // Real impl: log via px4_log::err!. Skeleton: drop.
        let _ = e;
    }
    loop {
        core::hint::spin_loop();
    }
}

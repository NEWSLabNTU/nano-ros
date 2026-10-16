//! phase-462 W3a -- the build-time FTTI check charges a rate-guarded fault two
//! of the executor's rate windows. The check (`nros-orchestration-ir`) cannot
//! depend on the `no_std` executor, so it restates the window; this test is
//! where the two crates meet and the restatement is held to the original.

use nros_node::executor::monitor::RATE_CHECK_INTERVAL_US;
use nros_orchestration_ir::ftti_budget::RATE_DETECTION_MS;

#[test]
fn the_rate_detection_bound_is_two_executor_rate_windows() {
    let window_ms = RATE_CHECK_INTERVAL_US as f64 / 1000.0;
    assert_eq!(
        RATE_DETECTION_MS,
        2.0 * window_ms,
        "ftti_budget::RATE_DETECTION_MS must be two of the executor's \
         RATE_CHECK_INTERVAL_US windows ({window_ms} ms each)"
    );
}

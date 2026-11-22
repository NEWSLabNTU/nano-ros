//! Issue 2732 -- the ThreadX byte pool refuses, at boot, a task stack its
//! derivation never counted.
//!
//! The pool's derived size counts the spawned sched tasks the image's sizing
//! descriptor states. A stack the build could not see (an owned entry, a bare
//! `cargo build`, a task spawned by hand) is not in it, and the refusal in
//! `nros_threadx_create_task` is what stands between that stack and a task
//! running past the pool. The fixture spawns one task no derived pool can
//! hold; this asserts the image says so, naming the bytes and the knob, and
//! that the task never ran.
//!
//! The BINARY name carries the `threadx` token, so tier 1's lane filter keeps
//! it off hosts whose build lane produces no threadx-linux fixture.

use nros_tests::{
    fixtures::{ManagedProcess, RequireFixture, build_sched_task_pool_refusal_threadx_linux},
    output::{
        SCHED_TASK_POOL_REFUSAL_ACCEPTED, SCHED_TASK_POOL_REFUSAL_SPAWNING,
        SCHED_TASK_POOL_REFUSAL_TASK_RAN, THREADX_POOL_REFUSAL_KNOB,
        THREADX_POOL_TASK_STACK_REFUSED,
    },
};
use std::{process::Command, time::Duration};

#[test]
fn threadx_pool_refuses_an_uncounted_task_stack_at_boot() {
    let bin = build_sched_task_pool_refusal_threadx_linux().require("sched-task-pool-refusal");

    let mut proc = ManagedProcess::spawn_image(Command::new(bin), "sched-task-pool-refusal")
        .expect("spawn the sched-task-pool-refusal image");
    let out = proc.collect_until(THREADX_POOL_REFUSAL_KNOB, Duration::from_secs(30));
    proc.kill();

    // The precondition: the image reached its spawn. Without it a refusal
    // missing below would be a boot that never got there, not a pool that
    // took the stack.
    assert!(
        out.contains(SCHED_TASK_POOL_REFUSAL_SPAWNING),
        "the image never reached its spawn. Full output:\n{out}"
    );
    assert!(
        out.contains(THREADX_POOL_TASK_STACK_REFUSED),
        "the byte pool did not refuse a 16 MiB task stack (issue 2732). Full output:\n{out}"
    );
    // The refusal states the requested bytes and names the knob.
    assert!(
        out.contains("16777216 bytes requested") && out.contains(THREADX_POOL_REFUSAL_KNOB),
        "the refusal does not name the request and the knob. Full output:\n{out}"
    );
    assert!(
        !out.contains(SCHED_TASK_POOL_REFUSAL_ACCEPTED)
            && !out.contains(SCHED_TASK_POOL_REFUSAL_TASK_RAN),
        "the image went on past the refusal. Full output:\n{out}"
    );
}

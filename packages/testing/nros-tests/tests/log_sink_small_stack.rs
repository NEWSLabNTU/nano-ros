//! A thread on a small stack can log (issue 2738).
//!
//! The Zephyr log sink formatted every `nros_log` record into a 1280-byte
//! buffer on the stack of the thread that logged, so a thread sized for its
//! own work overflowed the first time it logged, inside the dispatch. It now
//! formats into buffers the SINK owns (`CONFIG_NROS_ZEPHYR_LOG_SINK_BUFFERS`).
//!
//! The image (`bins/log-sink-small-stack-zephyr-smp`, `qemu_cortex_a53/smp`)
//! starts a thread on a 2048-byte stack that logs 50 records, then reports
//! its unused stack. Two builds, one image:
//!
//! - the pool (the default): every record reaches the console, the thread
//!   finishes, and no record took the truncating fallback;
//! - the old caller-stack buffer (`legacy-sink.conf`), the NEGATIVE CONTROL:
//!   the thread must NOT finish. If it does, the stack is large enough that
//!   this cell proves nothing about the pool, and it fails saying so.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west lane builds both leaves)
//!
//! Run with: `cargo nextest run -p nros-tests --test log_sink_small_stack`

use nros_tests::{
    fixtures::{
        RequireFixture, build_log_sink_small_stack_legacy_smp, build_log_sink_small_stack_smp,
    },
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::time::Duration;

const RECORDS: usize = 50;
const BOOT: Duration = Duration::from_secs(60);
const DONE: &str = "log-sink-small-stack: done";
const THREAD_DONE: &str = "log-sink-small-stack: thread done";

fn record(i: usize) -> String {
    format!("small-stack record {i}")
}

#[test]
fn a_small_stack_thread_logs_every_record_through_the_pool() {
    let binary = build_log_sink_small_stack_smp().require("the pool image");
    let proc = ZephyrProcess::start(binary, ZephyrPlatform::QemuCortexA53Smp)
        .expect("qemu_cortex_a53/smp boots");
    let out = proc.wait_for_pattern(DONE, BOOT);

    assert!(out.contains(DONE), "the image never finished:\n{out}");
    assert!(
        out.contains(THREAD_DONE),
        "the small-stack thread did not finish logging (overflowed?):\n{out}"
    );
    let missing: Vec<usize> = (0..RECORDS)
        .filter(|&i| !out.contains(&record(i)))
        .collect();
    assert!(
        missing.is_empty(),
        "records {missing:?} never reached the console:\n{out}"
    );
    assert!(
        out.contains("fallbacks 0"),
        "a record took the truncating fallback with one logging thread:\n{out}"
    );
    assert!(
        !out.contains("Stack overflow"),
        "a stack overflowed:\n{out}"
    );
}

#[test]
fn the_negative_control_overflows_on_the_callers_stack() {
    let binary = build_log_sink_small_stack_legacy_smp().require("the legacy-sink image");
    let proc = ZephyrProcess::start(binary, ZephyrPlatform::QemuCortexA53Smp)
        .expect("qemu_cortex_a53/smp boots");
    let out = proc.wait_for_pattern(THREAD_DONE, BOOT);

    assert!(
        out.contains("sink buffers 0"),
        "the control is not running the caller-stack sink:\n{out}"
    );
    assert!(
        !out.contains(THREAD_DONE),
        "the small-stack thread survived the 1280-byte caller-stack buffer, so this \
         cell proves nothing about the pool: shrink SMALL_STACK in the image\n{out}"
    );
}

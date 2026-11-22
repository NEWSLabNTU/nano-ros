//! A thread on a small stack can log (issue 2738).
//!
//! The Zephyr log sink formatted every `nros_log` record into a 1280-byte
//! buffer on the stack of the thread that logged, so a thread sized for its
//! own work overflowed the first time it logged, inside the dispatch. It now
//! formats into buffers the SINK owns (`CONFIG_NROS_ZEPHYR_LOG_SINK_BUFFERS`).
//!
//! The image (`bins/log-sink-small-stack-zephyr-smp`, `qemu_cortex_a53/smp`)
//! starts a thread that logs 50 records, then reports how many bytes of its
//! stack it used. The stack is large enough that neither sink overflows it,
//! so the verdict is a MEASUREMENT held to [`BUDGET`], not a crash. A crash
//! cannot be the verdict: the old sink's buffer is written from its low end,
//! past the stack sentinel, so on a 2 KiB stack it corrupted memory below the
//! stack and the thread usually finished anyway (measured on host 241). Two
//! builds, one image:
//!
//! - the pool (the default): every record reaches the console, no record took
//!   the truncating fallback, and the thread used at most [`BUDGET`] bytes;
//! - the old caller-stack buffer (`legacy-sink.conf`), the NEGATIVE CONTROL:
//!   the thread must use MORE than [`BUDGET`]. If it does not, the budget
//!   cannot tell the two sinks apart and the cell proves nothing.
//!
//! ## Prerequisites
//!
//! - `NROS_ZEPHYR_FIXTURE_FILTER=log-sink-small-stack just zephyr build-fixtures`
//!   (the west lane builds both leaves; `just ci gate` does not)
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

/// The stack a thread may spend logging, in bytes, including its own frame.
///
/// Measured on host 241 (Zephyr 3.7, `qemu_cortex_a53/smp`, 2026-11-20): the
/// pool used 2276, the old caller-stack sink 3332. The budget sits between
/// them with about 500 bytes of room on each side. If a toolchain or Zephyr
/// bump moves either number past it, re-measure both before moving it.
const BUDGET: usize = 2816;

/// `stack used <n> of <size>` from the thread-done line.
fn stack_used(out: &str) -> Option<usize> {
    let line = out.lines().find(|l| l.contains(THREAD_DONE))?;
    let rest = line.split("stack used ").nth(1)?;
    rest.split_whitespace().next()?.parse().ok()
}

/// `(sink buffers, cpus)` from the start line.
fn pool_shape(out: &str) -> Option<(usize, usize)> {
    let line = out.lines().find(|l| l.contains("sink buffers "))?;
    let buffers = line
        .split("sink buffers ")
        .nth(1)?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()?;
    let cpus = line
        .split("cpus ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((buffers, cpus))
}

/// The pool's default size for `cpus`: the `nros-knobs.toml` ladder for
/// `NROS_ZEPHYR_LOG_SINK_BUFFERS`, twice the CPUs in steps.
fn default_pool_for(cpus: usize) -> usize {
    match cpus {
        8.. => 16,
        4.. => 8,
        2.. => 4,
        _ => 2,
    }
}

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
    // The image states no pool size, so it runs the derived default.
    let (buffers, cpus) =
        pool_shape(&out).unwrap_or_else(|| panic!("no `sink buffers .., cpus ..` line:\n{out}"));
    assert!(cpus >= 2, "an SMP cell booted {cpus} CPU(s):\n{out}");
    assert_eq!(
        buffers,
        default_pool_for(cpus),
        "the pool is not the default for {cpus} CPUs:\n{out}"
    );
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
    let used = stack_used(&out).unwrap_or_else(|| panic!("no `stack used` figure:\n{out}"));
    // The figure, on a pass too, so a run records how close it came.
    eprintln!("log-sink pool: used={used} budget={BUDGET}");
    assert!(
        used <= BUDGET,
        "logging cost the thread {used} bytes of stack, over the {BUDGET}-byte budget:\n{out}"
    );
}

#[test]
fn the_negative_control_spends_more_than_the_budget() {
    let binary = build_log_sink_small_stack_legacy_smp().require("the legacy-sink image");
    let proc = ZephyrProcess::start(binary, ZephyrPlatform::QemuCortexA53Smp)
        .expect("qemu_cortex_a53/smp boots");
    let out = proc.wait_for_pattern(DONE, BOOT);

    assert!(
        out.contains("sink buffers 0"),
        "the control is not running the caller-stack sink:\n{out}"
    );
    let used = stack_used(&out)
        .unwrap_or_else(|| panic!("the control printed no `stack used` figure:\n{out}"));
    eprintln!("log-sink caller-stack control: used={used} budget={BUDGET}");
    assert!(
        used > BUDGET,
        "the caller-stack sink used {used} bytes, within the {BUDGET}-byte budget, so this \
         cell cannot tell it from the pool: re-measure both and move BUDGET\n{out}"
    );
}

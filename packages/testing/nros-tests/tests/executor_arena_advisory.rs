//! Issue 0900 — the executor arena is derived by budgeting EVERY slot at the
//! ActionClient worst case, so an image without one carries several times what
//! it can use. Two things are asserted here, both against a REAL opened
//! executor rather than by re-deriving the build-time constant, because the
//! point is what an image actually gets:
//!
//! 1. the gap itself (a timer-only executor claims 32 bytes of 74,240), and
//! 2. that the first-spin advisory REACHES A SINK, which is the whole value of
//!    W1 — a diagnostic nobody sees is the folklore it was meant to replace.
//!
//! # Why its own test binary
//!
//! The advisory is one-shot on a process-scoped flag (see
//! `executor::arena::ARENA_ADVISORY_DONE` — a static rather than an `Executor`
//! field, because a field would move `EXECUTOR_OPAQUE_U64S` and every image's
//! executor footprint to buy a diagnostic). Any other test in the same binary
//! that spins would consume it first, and Rust runs tests in parallel, so
//! sharing a binary would make this pass or fail by scheduling. The sink list
//! is global for the same reason.

#![cfg(feature = "component-runtime-test")]

use nros_rmw_zenoh as _;

use std::{sync::Mutex, time::Duration};

use nros::{Executor, ExecutorConfig, TimerDuration};
use nros_log::{LogSink, Record, init};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct CapturingSink;

impl LogSink for CapturingSink {
    fn log(&self, record: &Record<'_>) {
        // unwrap: poisoned only if another thread already panicked, which
        // would have failed the test anyway.
        CAPTURED.lock().unwrap().push(record.message.to_string());
    }
}

static SINK: CapturingSink = CapturingSink;
static SINKS: &[&dyn LogSink] = &[&SINK];

/// The advisory must be EMITTED and must REACH a sink.
///
/// `nros_log` holds records in an early ring while no sink is installed and
/// replays them on `init`, so a missing line here means the advisory did not
/// fire — not that logging swallowed it.
#[rstest]
fn first_spin_reports_an_over_provisioned_arena(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    init(SINKS);
    CAPTURED.lock().unwrap().clear();

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("arena_advisory")
        .domain_id(182);
    let mut executor = Executor::open(&cfg).expect("Executor::open failed");

    let capacity = executor.arena_capacity();
    assert!(
        capacity > 0,
        "a zero-capacity arena is issue 0460, not headroom"
    );

    executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .expect("register_timer");
    let used = executor.arena_used();

    assert!(
        used > 0,
        "registering a timer must claim arena bytes; a bump allocator that \
         charges nothing is not measuring anything"
    );
    // The defect, stated as a number. A timer-only image is the cheapest
    // possible workload and must not come near an arena sized for MAX_CBS
    // action clients. Expected to want rewriting once a per-kind derivation
    // lands — that is the point, it pins the number a fix has to move.
    assert!(
        used * 2 <= capacity,
        "issue 0900: a timer-only executor claimed {used} of {capacity} arena \
         bytes. If this FAILS because usage rose, the derivation may have been \
         fixed — re-read the issue before raising the threshold"
    );

    executor.spin_once(Duration::from_millis(10));
    let after_first = CAPTURED.lock().unwrap().len();
    // Spin again: the advisory names a BUILD-TIME constant, so saying it once
    // per process is the whole contract, and a per-spin log line on an RTOS
    // target is a flood (issue 0371's shape). Asserted here rather than in a
    // second test because the flag is process-scoped — a separate test would
    // observe an already-consumed flag and assert nothing, which is the vacuous
    // shape `check-no-vacuous-tests` exists to catch.
    executor.spin_once(Duration::from_millis(10));
    assert_eq!(
        CAPTURED.lock().unwrap().len(),
        after_first,
        "the arena advisory repeated on a second spin; it is one-shot"
    );

    let records = CAPTURED.lock().unwrap().clone();
    let advisory = records
        .iter()
        .find(|m| m.contains("arena over-provisioned"))
        .unwrap_or_else(|| {
            panic!(
                "the first-spin arena advisory reached no sink. {} record(s) \
                 captured: {records:?}",
                records.len()
            )
        });

    // It must carry the number to SET, not merely say the arena is big — the
    // knob it names has existed all along and went unset because nothing
    // computed a value for it (issues 0271/0739).
    assert!(
        advisory.contains("NROS_EXECUTOR_ARENA_SIZE="),
        "the advisory must name the value to set, not just report the waste: \
         {advisory}"
    );
    // `nros_log`'s call-site buffer is 256 bytes by default and truncates with
    // a `…`. The first version of this line ran ~450 bytes, so the sink got the
    // whole explanation and none of the value. Guard the budget, not just the
    // wording — an advisory cut before its actionable half is worse than none,
    // because it reads as though it helped.
    assert!(
        !advisory.contains('\u{2026}'),
        "the advisory overflowed nros_log's format buffer and was truncated; \
         shorten it or the value to set never reaches the user: {advisory}"
    );
}

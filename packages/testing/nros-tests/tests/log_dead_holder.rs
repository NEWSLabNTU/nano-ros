//! A thread that dies holding nros_log's dispatcher is reported, and later
//! records are delivered (issue 2738).
//!
//! nros_log delivers one record at a time behind a process-wide guard, and a
//! record that meets the guard held is handed to the holder to deliver. A
//! thread that faulted inside a sink and was aborted held it for good, so
//! every later record from every thread was held for a thread that never
//! delivered it, and nothing said so. The Zephyr port's
//! `nros_zephyr_log_dead_holder_check`, called from a fatal error handler,
//! now names the dying thread once and hands the dispatcher back.
//!
//! The image (`bins/log-dead-holder-zephyr-smp`, `qemu_cortex_a53/smp`)
//! installs a C sink that `k_oops`es on a `DIE` record and a fatal handler
//! that RETURNS (so Zephyr aborts the thread and runs on). In one boot:
//!
//! - phase 1: `victim1` dies in the sink, the handler runs the check: one
//!   report naming `victim1`, and main's next record reaches the console;
//! - phase 2, the NEGATIVE CONTROL: `victim2` dies the same way and the
//!   handler skips the check: no report, and main's next record never
//!   arrives. Without it, phase 1's delivery could be a guard that was never
//!   held.
//!
//! ## Prerequisites
//!
//! - `NROS_ZEPHYR_FIXTURE_FILTER=log-dead-holder just zephyr build-fixtures`
//!   (the west lane builds the leaf; `just ci gate` does not)
//!
//! Run with: `cargo nextest run -p nros-tests --test log_dead_holder`

use nros_tests::{
    fixtures::{RequireFixture, build_log_dead_holder_smp},
    output::{
        LOG_DEAD_HOLDER_AFTER_RECORD, LOG_DEAD_HOLDER_DONE, LOG_DEAD_HOLDER_HANDLER_RETURNS,
        LOG_DEAD_HOLDER_PHASE1_DONE, LOG_DEAD_HOLDER_REPORT, LOG_DEAD_HOLDER_STRANDED_RECORD,
    },
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::time::Duration;

const BOOT: Duration = Duration::from_secs(60);

#[test]
fn a_dead_dispatcher_holder_is_reported_once_and_later_records_are_delivered() {
    let binary = build_log_dead_holder_smp().require("the dead-holder image");
    let proc = ZephyrProcess::start(binary, ZephyrPlatform::QemuCortexA53Smp)
        .expect("qemu_cortex_a53/smp boots");
    let out = proc.wait_for_pattern(LOG_DEAD_HOLDER_DONE, BOOT);

    assert!(
        out.contains(LOG_DEAD_HOLDER_DONE),
        "the image never finished:\n{out}"
    );
    assert_eq!(
        out.matches(LOG_DEAD_HOLDER_HANDLER_RETURNS).count(),
        2,
        "both victims must die in the faulting sink, once each:\n{out}"
    );

    // Phase 1: one report, naming the thread that died, and delivery resumes.
    let phase1 = out
        .split(LOG_DEAD_HOLDER_PHASE1_DONE)
        .next()
        .unwrap_or_default();
    let reports: Vec<&str> = out
        .lines()
        .filter(|l| l.contains(LOG_DEAD_HOLDER_REPORT))
        .collect();
    assert_eq!(
        reports.len(),
        1,
        "exactly one death is checked, so exactly one report:\n{out}"
    );
    assert!(
        reports[0].contains("victim1"),
        "the report must name the thread that died: {}\n{out}",
        reports[0]
    );
    assert!(
        phase1.contains(LOG_DEAD_HOLDER_AFTER_RECORD),
        "the record after the released death never reached the console:\n{out}"
    );

    // Phase 2, the negative control: the same death unchecked strands the
    // next record, so phase 1's delivery is the release's doing.
    assert!(
        !out.contains(LOG_DEAD_HOLDER_STRANDED_RECORD),
        "a record after an UNCHECKED death was delivered, so the dispatcher was never \
         held and phase 1 proves nothing:\n{out}"
    );
}

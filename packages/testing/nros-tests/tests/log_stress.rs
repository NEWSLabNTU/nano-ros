//! Concurrent loggers lose no record on the Zephyr sink (issue 2762).
//!
//! nros_log used to deliver one record at a time behind a dispatch guard, and a
//! record that found the guard held waited in a hold queue of 4, so on an SMP
//! image most records from concurrent loggers were lost. A sink that declares
//! itself re-entrant now gets each record directly, in the context that raised
//! it; the Zephyr console sink does.
//!
//! The image (`bins/log-stress-zephyr-smp`, `qemu_cortex_a53/smp`) releases 16
//! logger threads at once, 200 records each, while a timer interrupt logs 100
//! more: well above the sink's buffer pool (2 per CPU). In ONE boot:
//!
//! - the console (re-entrant) must get every record. A record that met a full
//!   pool takes the truncating fallback, which delivers it marked and is
//!   counted (`fallbacks`); that is not a loss;
//! - the NEGATIVE CONTROL, a serialised C sink installed with
//!   `rclc_log_add_sink`, holds the guard briefly per record and must get
//!   FEWER than all of them. If it got every one the run never contended, and
//!   the console's full count would prove nothing.
//!
//! Zephyr's own deferred buffer is sized for the whole run, but it can still
//! drop the odd message (an interrupt's record cannot wait for room), and it
//! says how many (`--- N messages dropped ---`). Those are Zephyr's; a console
//! record missing beyond them is nros_log's, and fails the cell.
//!
//! ## Prerequisites
//!
//! - `NROS_ZEPHYR_FIXTURE_FILTER=log-stress just zephyr build-fixtures`
//!
//! Run with: `cargo nextest run -p nros-tests --test log_stress`

use nros_tests::{
    fixtures::{RequireFixture, build_log_stress_smp},
    output::{
        LOG_STRESS_DONE, LOG_STRESS_ISR_RECORD, LOG_STRESS_START, LOG_STRESS_THREAD_RECORD,
        ZEPHYR_LOG_MESSAGES_DROPPED,
    },
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::{collections::BTreeSet, time::Duration};

const BOOT: Duration = Duration::from_secs(120);

/// `<key> <n>` out of a `, `-separated summary line.
fn field(line: &str, key: &str) -> Option<usize> {
    line.split(", ")
        .find_map(|part| part.trim().strip_prefix(key)?.trim().parse().ok())
}

/// The line starting with `marker`, from `marker` on.
fn line_from<'a>(out: &'a str, marker: &str) -> Option<&'a str> {
    out.lines()
        .find_map(|l| l.find(marker).map(|i| &l[i + marker.len()..]))
}

/// The messages Zephyr's deferred log core says it dropped: the sum of every
/// `--- N messages dropped ---` line.
fn zephyr_dropped(out: &str) -> usize {
    out.lines()
        .filter(|l| l.contains(ZEPHYR_LOG_MESSAGES_DROPPED))
        .filter_map(|l| {
            let head = l.split(ZEPHYR_LOG_MESSAGES_DROPPED).next()?;
            head.split_whitespace().last()?.parse::<usize>().ok()
        })
        .sum()
}

/// The distinct `(thread, record)` pairs and interrupt records the console got.
fn delivered(out: &str) -> (BTreeSet<(usize, usize)>, BTreeSet<usize>) {
    let mut threads = BTreeSet::new();
    let mut isr = BTreeSet::new();
    for line in out.lines() {
        if let Some(rest) = line_from(line, LOG_STRESS_THREAD_RECORD) {
            // `NN rNNNN`
            let mut it = rest.split_whitespace();
            let t = it.next().and_then(|t| t.parse().ok());
            let r = it
                .next()
                .and_then(|r| r.strip_prefix('r'))
                .and_then(|r| r.get(..4))
                .and_then(|r| r.parse().ok());
            if let (Some(t), Some(r)) = (t, r) {
                threads.insert((t, r));
            }
        } else if let Some(rest) = line_from(line, LOG_STRESS_ISR_RECORD)
            && let Some(r) = rest.get(..4).and_then(|r| r.parse().ok())
        {
            isr.insert(r);
        }
    }
    (threads, isr)
}

#[test]
fn concurrent_loggers_and_an_interrupt_lose_no_record() {
    let binary = build_log_stress_smp().require("the log stress image");
    let proc = ZephyrProcess::start(binary, ZephyrPlatform::QemuCortexA53Smp)
        .expect("qemu_cortex_a53/smp boots");
    let out = proc.wait_for_pattern(LOG_STRESS_DONE, BOOT);

    let start = line_from(&out, LOG_STRESS_START)
        .unwrap_or_else(|| panic!("no `{LOG_STRESS_START}` line:\n{out}"));
    let shape = |key: &str| {
        field(start, key).unwrap_or_else(|| panic!("no `{key}` in the start line `{start}`"))
    };
    let (loggers, records, isr_records) =
        (shape("loggers"), shape("records"), shape("isr records"));
    let (buffers, cpus) = (shape("sink buffers"), shape("cpus"));
    assert!(cpus >= 2, "an SMP cell booted {cpus} CPU(s):\n{out}");
    assert!(
        loggers > buffers,
        "{loggers} loggers do not outnumber the {buffers}-buffer pool, so this is no stress"
    );

    let done = line_from(&out, LOG_STRESS_DONE)
        .unwrap_or_else(|| panic!("the image never finished:\n{out}"));
    let total = field(done, "records").expect("records in the summary");
    let serial = field(done, "serialised sink got").expect("the serialised sink's count");
    let fallbacks = field(done, "fallbacks").expect("fallbacks in the summary");
    assert_eq!(total, loggers * records + isr_records, "summary: `{done}`");
    // The figures, on a pass too, so a run records how hard it contended.
    eprintln!(
        "log stress: {total} records, serialised sink got {serial}, console fallbacks {fallbacks}"
    );

    // Zephyr's own deferred buffer can still drop a message now and then (an
    // interrupt's record cannot wait for room), and says how many. Those are
    // Zephyr's; a console record missing beyond them is nros_log's.
    let zephyr_dropped = zephyr_dropped(&out);

    let (threads, isr) = delivered(&out);
    let missing_threads: Vec<(usize, usize)> = (0..loggers)
        .flat_map(|t| (0..records).map(move |r| (t, r)))
        .filter(|pair| !threads.contains(pair))
        .collect();
    let missing_isr: Vec<usize> = (0..isr_records).filter(|r| !isr.contains(r)).collect();
    let missing = missing_threads.len() + missing_isr.len();
    eprintln!("log stress: console missing {missing}, Zephyr reported {zephyr_dropped} dropped");
    assert!(
        missing <= zephyr_dropped,
        "the re-entrant console sink lost {} thread record(s) (first {:?}) and {} interrupt \
         record(s) (first {:?}) of {total}; Zephyr's own buffer reported only \
         {zephyr_dropped} dropped, so nros_log lost the rest",
        missing_threads.len(),
        missing_threads.iter().take(8).collect::<Vec<_>>(),
        missing_isr.len(),
        missing_isr.iter().take(8).collect::<Vec<_>>(),
    );

    // The negative control: the serialised path, in the same run, must lose.
    assert!(
        serial < total,
        "the serialised sink got all {total} records, so the run never contended and the \
         console's full count proves nothing: raise the loggers or the sink's hold time"
    );
}

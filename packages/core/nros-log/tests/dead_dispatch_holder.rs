//! Issue 2738 — a thread that dies holding the dispatch guard strands every
//! later record, and a fault handler that knows it can hand the guard back.
//!
//! Its own test binary: `init` publishes process-global state, and the
//! dispatch guard is process-wide.
//!
//! The "death" is a thread that parks forever inside a sink: to `nros_log` it
//! is indistinguishable from one a kernel aborted there, which is the point —
//! nothing in this crate can tell, so the platform's fault handler must.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nros_log::{LogSink, Logger, Record, Severity, init};

static IN_SINK: AtomicBool = AtomicBool::new(false);
/// An address on the dying thread's stack, taken inside the sink.
static DYING_STACK: AtomicUsize = AtomicUsize::new(0);

fn wait_for(flag: &AtomicBool) -> bool {
    let start = Instant::now();
    while !flag.load(Ordering::SeqCst) {
        if start.elapsed() > Duration::from_secs(5) {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

struct Fatal {
    lines: Mutex<Vec<String>>,
}

impl LogSink for Fatal {
    fn log(&self, record: &Record<'_>) {
        if record.message == "die" {
            let here = 0u8;
            DYING_STACK.store(core::ptr::addr_of!(here) as usize, Ordering::SeqCst);
            IN_SINK.store(true, Ordering::SeqCst);
            // Never returns: the thread "died" holding the guard.
            loop {
                std::thread::park();
            }
        }
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record.message.to_string());
    }
}

static FATAL: Fatal = Fatal {
    lines: Mutex::new(Vec::new()),
};
static SINKS: [&dyn LogSink; 1] = [&FATAL];
static LOGGER: Logger = Logger::with_level("dead_holder", Severity::Trace);

fn lines() -> Vec<String> {
    FATAL
        .lines
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[test]
fn a_dead_holder_strands_records_until_its_guard_is_released() {
    init(&SINKS);

    nros_log::log_info!(&LOGGER, "before");
    assert_eq!(
        nros_log::dispatch_holder_frame(),
        0,
        "a finished delivery must leave no holder"
    );

    // A detached thread: it never finishes, as an aborted one never would.
    let _dying = std::thread::spawn(|| nros_log::log_info!(&LOGGER, "die"));
    assert!(
        wait_for(&IN_SINK),
        "the dying record never reached the sink"
    );

    let frame = nros_log::dispatch_holder_frame();
    assert_ne!(frame, 0, "a held guard must name its holder");
    // The holder's frame is on the dying thread's stack, a few frames above the
    // sink: what a fault handler compares against that thread's stack bounds.
    let dying = DYING_STACK.load(Ordering::SeqCst);
    assert!(
        frame.abs_diff(dying) < 64 * 1024,
        "holder frame {frame:#x} is not on the dying thread's stack (sink at {dying:#x})"
    );

    // While the holder is "dead", a record is held for it, never delivered.
    nros_log::log_info!(&LOGGER, "stranded");
    assert_eq!(
        lines(),
        vec!["before".to_string()],
        "nothing can be delivered"
    );

    // A handler that names the wrong frame releases nothing (the guard may be
    // a live thread's).
    assert!(!nros_log::release_dead_dispatch_holder(frame + 1));
    assert!(!nros_log::release_dead_dispatch_holder(0));
    assert_eq!(nros_log::dispatch_holder_frame(), frame);

    // The fault handler that knows the holder is dead hands the guard back,
    // once.
    assert!(nros_log::release_dead_dispatch_holder(frame));
    assert!(!nros_log::release_dead_dispatch_holder(frame));

    nros_log::log_info!(&LOGGER, "after");
    let got = lines();
    assert!(
        got.contains(&"after".to_string()) && got.contains(&"stranded".to_string()),
        "after the release both the new and the held record are delivered: {got:?}"
    );
    assert_eq!(nros_log::dispatch_holder_frame(), 0);
    assert_eq!(nros_log::early::overflowed(), 0);
}

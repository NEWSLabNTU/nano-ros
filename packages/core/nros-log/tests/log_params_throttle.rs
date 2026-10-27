//! phase-515 R3 — `log_info!(logger.throttle(d), …)`, rclrs's throttle
//! modifier. It reads the platform clock, so it exists only under
//! `cfg(nros_log_clock)`; this target is built with `platform-clock` and
//! supplies `nros_platform_clock_ns` itself, a clock the test moves by hand.
//! Run by `just check required-features-tests`.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nros_log::{LogSink, Logger, Record, Severity, ThrottleClock, ToLogParams, log_info};

static NOW_NS: AtomicU64 = AtomicU64::new(1_000_000_000);

#[unsafe(no_mangle)]
extern "C" fn nros_platform_clock_ns() -> u64 {
    NOW_NS.load(Ordering::SeqCst)
}

struct Collector {
    lines: Mutex<Vec<(String, String, u64)>>,
}

impl LogSink for Collector {
    fn log(&self, record: &Record<'_>) {
        self.lines.lock().unwrap().push((
            record.logger_name.to_string(),
            record.message.to_string(),
            record.timestamp_ns,
        ));
    }
}

static COLLECTOR: Collector = Collector {
    lines: Mutex::new(Vec::new()),
};

fn advance(ms: u64) {
    NOW_NS.fetch_add(ms * 1_000_000, Ordering::SeqCst);
}

#[test]
fn throttle_admits_one_record_per_window_on_the_platform_clock() {
    assert!(nros_log::add_sink(&COLLECTOR));
    static L: Logger = Logger::with_level("r3_throttle", Severity::Debug);
    let tick = |i: u32| {
        log_info!(
            L.throttle(Duration::from_millis(100))
                .throttle_clock(ThrottleClock::SteadyTime),
            "tick {i}"
        )
    };
    tick(0); // first record of the site: emits
    advance(40);
    tick(1); // 40 ms in: suppressed
    advance(50);
    tick(2); // 90 ms in: suppressed
    advance(10);
    tick(3); // 100 ms after tick 0: emits, and opens the next window
    advance(99);
    tick(4); // suppressed
    advance(1);
    tick(5); // emits

    let lines = COLLECTOR.lines.lock().unwrap();
    let got: Vec<(&str, u64)> = lines
        .iter()
        .filter(|(name, ..)| name == "r3_throttle")
        .map(|(_, m, t)| (m.as_str(), *t))
        .collect();
    let base = 1_000_000_000;
    assert_eq!(
        got,
        [
            ("tick 0", base),
            ("tick 3", base + 100_000_000),
            ("tick 5", base + 200_000_000)
        ],
        "one record per 100 ms window, stamped with the reading that decided it"
    );
}

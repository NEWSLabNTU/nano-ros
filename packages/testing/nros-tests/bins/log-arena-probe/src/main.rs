//! phase-479 W5 (RFC-0102 D5) — the runtime-logger arena knob, observed.
//!
//! Built for the `native` board, which states 32, with an image env that states
//! `NROS_LOG_DYNAMIC_LOGGERS=24`. It creates `LOGGERS` runtime loggers, stamps
//! the boot record, prints one line naming the capacity and the slots in use,
//! and then waits for stdin to close so `tests/log_arena_knob.rs` can read the
//! boot record out of `/proc/<pid>/mem` while the process is alive.
//!
//! issue 1037 — the same line reads back the rest of the `[knobs.log]` tenant
//! (ceiling, formatting buffer, early ring, `/rosout` depth) and whether the
//! platform clock was compiled in, which here comes from the `posix` platform's
//! `[capabilities] clock = true` and NOT from a feature: this leaf names none.

use std::io::Read;

use nros_log::Severity;
use nros_node::boot_report::{self, Stage};

/// Runtime loggers this probe creates: a node-like parent and its children,
/// the shapes RFC-0102 D5 counts against the arena.
const LOGGERS: &[&str] = &["probe", "probe.a", "probe.b", "probe.c", "probe.d"];

extern crate nros_platform_cffi as _;

fn main() {
    boot_report::init();
    for name in LOGGERS {
        if nros_log::get_or_create_logger(name).is_none() {
            eprintln!("log-arena-probe: could not create `{name}`");
            std::process::exit(1);
        }
    }
    boot_report::checkpoint(Stage::FirstSpin);
    // The ceiling as the index of the least severe level that passes (6 = off).
    let max_level = [
        Severity::Trace,
        Severity::Debug,
        Severity::Info,
        Severity::Warn,
        Severity::Error,
        Severity::Fatal,
    ]
    .iter()
    .position(|s| nros_log::severity_enabled_at_compile_time(*s))
    .unwrap_or(6);
    // ONE line: the test reads one line and then closes its end of the pipe,
    // so a second `println!` would die on EPIPE and take the boot record with it.
    println!(
        "log-arena-probe: capacity={} in_use={} max_level={max_level} buffer={} early={} \
         rosout={} clock={}",
        nros_log::dynamic_logger_capacity(),
        nros_log::dynamic_loggers_in_use(),
        nros_log::format_buffer_capacity(),
        nros_log::early::early_depth(),
        nros_log::rosout::rosout_depth(),
        u8::from(nros_log::timestamp_available()),
    );
    // Keep the process (and its boot record) alive until the test is done.
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
}

//! Port conformance benchmark — the same primitives, timed on every port.
//!
//! nano-ros supports Zephyr, FreeRTOS, ThreadX, NuttX, POSIX, ESP-IDF and
//! several bare-metal boards. There is no way to say what a port COSTS, or to
//! notice when one regresses. The existing benches under `nros-bench` measure
//! the middleware on one target apiece (`wcet-cycles-qemu` times publish and
//! serialize through the DWT counter, `wake-latency-cortex-m3` times a wake);
//! none of them compares ports, because none of them measures the surface the
//! ports actually implement.
//!
//! This one times the `nros_platform_*` ABI directly, which is the only thing
//! every port has in common. The shape is EEMBC's Thread-Metric suite reduced
//! to the primitives this ABI exposes:
//!
//!   Thread-Metric               here
//!   memory alloc/dealloc        alloc+free
//!   cooperative switch          yield
//!   semaphore processing        mutex lock+unlock
//!
//! The absolute numbers are board-specific and not the point. The point is the
//! per-port table and the regression signal: a port that halves its alloc rate
//! between two pins should say so.
//!
//! `src/bench.rs` carries no target, allocator or std dependency, so a board
//! runner calls exactly the same code this host runner does. A number is only
//! comparable across ports if the thing being timed is identical.
//!
//! Usage: `cargo run --release` (host/POSIX). For a target, write a runner
//! beside this one that supplies an entry point and a clock, and depend on
//! that port's platform crate instead of `posix-c-port`.

mod bench;

// Force-link the POSIX C port. Nothing in this crate NAMES the cffi crate --
// the bench calls the `nros_platform_*` symbols directly through `extern "C"`
// -- so without this the linker drops the dependency and every symbol comes
// back undefined. `executor-fairness` gets the same crate transitively
// through `nros` and so never had to say it out loud.
use nros_platform_cffi as _;

/// Host clock. A target runner supplies its own -- a DWT cycle counter scaled
/// to microseconds, a kernel tick, whatever the board has -- which is why
/// `bench` takes the clock as a parameter rather than reaching for one.
fn clock_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn main() {
    // 200 ms per test: long enough that scheduler noise averages out, short
    // enough that the whole suite is under a second in CI.
    let budget_us: u64 = std::env::var("NROS_BENCH_BUDGET_US")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);

    println!("nros port-metric — platform ABI primitives");
    println!(
        "port: {}",
        std::env::var("NROS_BENCH_PORT").unwrap_or_else(|_| "posix".into())
    );
    println!("budget: {budget_us} us per test\n");
    println!("{:<20} {:>14} {:>16}", "test", "iterations", "per second");
    println!("{}", "-".repeat(52));

    let mut unsupported = 0;
    for o in bench::all(budget_us, clock_us) {
        if o.supported {
            println!("{:<20} {:>14} {:>16}", o.name, o.iterations, o.per_second());
        } else {
            // Named, not omitted: a missing row reads as "not measured yet",
            // which is a different claim from "this port does not provide it".
            println!("{:<20} {:>14} {:>16}", o.name, "-", "unsupported");
            unsupported += 1;
        }
    }

    if unsupported > 0 {
        println!("\n{unsupported} primitive(s) unsupported on this port.");
    }
}

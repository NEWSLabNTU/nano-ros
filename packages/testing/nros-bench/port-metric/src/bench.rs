//! The measurements themselves. Deliberately free of any target, allocator
//! or std dependency so a board runner can call exactly the same code the
//! host runner does — a number is only comparable across ports if the thing
//! being timed is identical.

use core::ffi::c_void;

/// One test's result. Iterations rather than a duration, because the point
/// is comparison across ports whose clocks differ in resolution and cost.
pub struct Outcome {
    pub name: &'static str,
    pub iterations: u64,
    pub elapsed_us: u64,
    /// `false` when the port does not provide the primitive. Reported rather
    /// than skipped: "this port cannot do it" is a result, and omitting the
    /// row would read as "not measured yet".
    pub supported: bool,
}

impl Outcome {
    pub fn per_second(&self) -> u64 {
        if self.elapsed_us == 0 {
            return 0;
        }
        self.iterations.saturating_mul(1_000_000) / self.elapsed_us
    }
}

/// Clock the harness supplies. Taken as a parameter rather than reached for
/// directly: `nros_platform_time_now_ns` returns 0 on ports with no RTC, and
/// a bench that silently divided by that would report nonsense.
pub type ClockUs = fn() -> u64;

/// Run `body` until `budget_us` has elapsed, returning the iteration count.
///
/// A fixed time budget rather than a fixed iteration count, so a slow port
/// finishes in the same wall time as a fast one. The clock is read once per
/// batch of `BATCH` iterations, not per iteration, so the measurement is of
/// the primitive and not of the clock.
fn run_for(budget_us: u64, clock: ClockUs, mut body: impl FnMut()) -> (u64, u64) {
    const BATCH: u64 = 64;
    let start = clock();
    let mut iterations = 0u64;
    loop {
        for _ in 0..BATCH {
            body();
        }
        iterations += BATCH;
        let elapsed = clock().saturating_sub(start);
        if elapsed >= budget_us {
            return (iterations, elapsed);
        }
    }
}

/// Allocate then free one small block.
///
/// EEMBC Thread-Metric calls this "memory allocation"; here it is the
/// `nros_platform_alloc`/`dealloc` pair every port must supply. A port whose
/// allocator is a bump arena and one with a real free list differ by an order
/// of magnitude, and nothing currently says so.
pub fn alloc_free(budget_us: u64, clock: ClockUs) -> Outcome {
    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut c_void;
        fn nros_platform_dealloc(ptr: *mut c_void);
    }
    let mut ok = true;
    let (iterations, elapsed_us) = run_for(budget_us, clock, || {
        // SAFETY: a 64-byte request and its matching free; the pointer never
        // escapes this closure.
        unsafe {
            let p = nros_platform_alloc(64);
            if p.is_null() {
                ok = false;
                return;
            }
            nros_platform_dealloc(p);
        }
    });
    Outcome {
        name: "alloc+free",
        iterations,
        elapsed_us,
        supported: ok,
    }
}

/// Yield to the scheduler and come back.
///
/// Thread-Metric's cooperative context switch, reduced to the one primitive
/// the ABI exposes. On a cooperative single-thread executor this is the cost
/// of the loop deciding to let something else run.
pub fn yield_cost(budget_us: u64, clock: ClockUs) -> Outcome {
    unsafe extern "C" {
        fn nros_platform_yield_now();
    }
    let (iterations, elapsed_us) = run_for(budget_us, clock, || {
        // SAFETY: no arguments, no state.
        unsafe { nros_platform_yield_now() }
    });
    Outcome {
        name: "yield",
        iterations,
        elapsed_us,
        supported: true,
    }
}

/// Uncontended mutex lock/unlock.
///
/// Thread-Metric's "semaphore processing". Uncontended deliberately: a
/// contended number measures the scheduler's wake path, which
/// `wake-latency-cortex-m3` already covers, whereas this is the cost every
/// guarded access pays whether or not anyone is waiting.
pub fn mutex_uncontended(budget_us: u64, clock: ClockUs) -> Outcome {
    unsafe extern "C" {
        fn nros_platform_mutex_storage_size() -> usize;
        fn nros_platform_mutex_init(m: *mut c_void) -> i8;
        fn nros_platform_mutex_lock(m: *mut c_void) -> i8;
        fn nros_platform_mutex_unlock(m: *mut c_void) -> i8;
        fn nros_platform_mutex_drop(m: *mut c_void) -> i8;
        fn nros_platform_alloc(size: usize) -> *mut c_void;
        fn nros_platform_dealloc(ptr: *mut c_void);
    }
    // SAFETY: storage sized by the port's own probe, initialised before use
    // and dropped after.
    unsafe {
        let size = nros_platform_mutex_storage_size();
        if size == 0 {
            return Outcome {
                name: "mutex lock+unlock",
                iterations: 0,
                elapsed_us: 0,
                supported: false,
            };
        }
        let m = nros_platform_alloc(size);
        if m.is_null() || nros_platform_mutex_init(m) != 0 {
            if !m.is_null() {
                nros_platform_dealloc(m);
            }
            return Outcome {
                name: "mutex lock+unlock",
                iterations: 0,
                elapsed_us: 0,
                supported: false,
            };
        }
        let (iterations, elapsed_us) = run_for(budget_us, clock, || {
            nros_platform_mutex_lock(m);
            nros_platform_mutex_unlock(m);
        });
        nros_platform_mutex_drop(m);
        nros_platform_dealloc(m);
        Outcome {
            name: "mutex lock+unlock",
            iterations,
            elapsed_us,
            supported: true,
        }
    }
}

/// Every test, in a fixed order so two runs diff cleanly.
pub fn all(budget_us: u64, clock: ClockUs) -> [Outcome; 3] {
    [
        alloc_free(budget_us, clock),
        yield_cost(budget_us, clock),
        mutex_uncontended(budget_us, clock),
    ]
}

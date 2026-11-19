//! Issue 2706 -- an interrupt wakes a parked executor through
//! `GuardCondition::trigger_from_isr`, on NuttX on the QEMU ARM `virt`
//! machine.
//!
//! The interrupt is a NuttX watchdog, whose function runs from the timer
//! interrupt: the board's `nros_board_nuttx_isr_timer_start` re-arms it every
//! [`PERIOD_US`] and calls [`on_interrupt`]. The board counts fires that ran
//! outside interrupt context, and the image prints that count, so a run that
//! tested a thread wake cannot pass.
//!
//! Two phases, because the NuttX signal reaches two kinds of park:
//!
//! 1. The executor. A guard condition is triggered from the interrupt while
//!    the executor parks in `spin_once` with a 1 s budget and nothing else to
//!    do; this is a SEMAPHORE wait. Each callback prints
//!    `ISR_WAKE <latency_us>`, then `ISR_WAKE_DONE <fires> <callbacks>
//!    <refused>`.
//! 2. The HANDLE park (`nros_platform_wake_park_handles_us`), the park a sched
//!    task whose session is a file descriptor takes. It wakes on the wake
//!    object's pipe, which an interrupt cannot `write`, so the port queues the
//!    byte on the work queue. The interrupt signals the wake object directly
//!    [`PARK_ROUNDS`] times while the main thread parks on it with no handles
//!    and a 1 s deadline; each round prints
//!    `ISR_PARK <latency_us> <rc> <parked_us> <interrupts>`, then
//!    `ISR_PARK_DONE <woken> <refused>`.
//!
//! Latencies are read off the ARM generic timer and printed SIGNED, so a wake
//! observed before its interrupt's stamp shows as negative.
//!
//! Then `ISR_WAKE_NOT_IN_ISR <n>`, and the image exits.

#![no_std]
#![no_main]

use core::fmt::Write as _;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use nros::{Executor, ExecutorConfig, GuardCondition};
use nros_board_nuttx_qemu as _;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut buf = Line::new();
    let _ = write!(buf, "{info}");
    unsafe extern "C" {
        fn nros_platform_panic(msg: *const u8, len: usize) -> !;
    }
    // SAFETY: `buf.bytes[..used]` is initialised and outlives the call.
    unsafe { nros_platform_panic(buf.bytes.as_ptr(), buf.used) }
}

/// The router this image dials: `nros_tests::alloc::NUTTX_ISR_WAKE_PORT`, as
/// in `system.toml`.
const LOCATOR: &str = "tcp/10.0.2.2:8500";
/// The interrupt's period.
const PERIOD_US: u32 = 20_000;
/// How many executor callbacks the run collects.
const SAMPLES: u32 = 50;
/// How many handle-park rounds the run makes.
const PARK_ROUNDS: u32 = 10;

/// What the interrupt does: 1 triggers the guard, 2 signals the raw wake
/// object, 0 nothing.
static MODE: AtomicU32 = AtomicU32::new(0);
/// The guard the interrupt triggers, leaked once set.
static GUARD: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// The raw wake object of the handle-park phase.
static WAKE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
/// Interrupts that triggered or signalled.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose signal reported that the wake reached nothing.
static REFUSED: AtomicU32 = AtomicU32::new(0);
/// `clock_ns` at the last trigger or signal.
static FIRED_AT: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn nros_board_nuttx_isr_timer_start(
        period_us: u32,
        cb: extern "C" fn(*mut core::ffi::c_void),
        arg: *mut core::ffi::c_void,
    ) -> i32;
    fn nros_board_nuttx_isr_timer_stop();
    fn nros_board_nuttx_isr_timer_not_in_isr() -> u32;
    fn nros_board_nuttx_isr_timer_now_ns() -> u64;
    fn nros_platform_wake_storage_size() -> usize;
    fn nros_platform_wake_storage_align() -> usize;
    fn nros_platform_wake_init(w: *mut core::ffi::c_void) -> i8;
    fn nros_platform_wake_signal_from_isr(w: *mut core::ffi::c_void) -> i8;
    fn nros_platform_wake_park_handles_us(
        w: *mut core::ffi::c_void,
        handles: *const i32,
        n: usize,
        deadline_us: u64,
    ) -> i8;
    fn write(fd: i32, buf: *const u8, len: usize) -> isize;
}

/// The ARM generic timer, through the board: `clock_ns` counts whole 1 ms
/// ticks here, coarser than the latency this image measures (issue 2748).
fn now_ns() -> u64 {
    // SAFETY: reads a counter register; no state.
    unsafe { nros_board_nuttx_isr_timer_now_ns() }
}

/// One console line, formatted without allocating and written with one
/// `write(1, ...)`.
struct Line {
    bytes: [u8; 192],
    used: usize,
}

impl Line {
    const fn new() -> Self {
        Self {
            bytes: [0u8; 192],
            used: 0,
        }
    }
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = self.bytes.len() - self.used;
        let n = s.len().min(room);
        self.bytes[self.used..self.used + n].copy_from_slice(&s.as_bytes()[..n]);
        self.used += n;
        Ok(())
    }
}

macro_rules! out {
    ($($arg:tt)*) => {{
        let mut line = Line::new();
        let _ = writeln!(line, $($arg)*);
        // SAFETY: stdout, and `line.bytes[..used]` is initialised.
        let _ = unsafe { write(1, line.bytes.as_ptr(), line.used) };
    }};
}

/// Runs in the timer interrupt (the board's watchdog function).
extern "C" fn on_interrupt(_arg: *mut core::ffi::c_void) {
    let ok = match MODE.load(Ordering::Acquire) {
        1 => {
            let g = GUARD.load(Ordering::Acquire);
            if g.is_null() {
                return;
            }
            FIRED_AT.store(now_ns(), Ordering::Release);
            // SAFETY: a leaked handle, set before the mode was.
            unsafe { &*g }.trigger_from_isr()
        }
        2 => {
            let w = WAKE.load(Ordering::Acquire);
            if w.is_null() {
                return;
            }
            FIRED_AT.store(now_ns(), Ordering::Release);
            // SAFETY: an initialised wake object that is never dropped.
            let rc = unsafe { nros_platform_wake_signal_from_isr(w) };
            rc == 0
        }
        _ => return,
    };
    FIRES.fetch_add(1, Ordering::Relaxed);
    if !ok {
        REFUSED.fetch_add(1, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    nros_rmw_zenoh::register().expect("register the zenoh backend");
    let exec_config = ExecutorConfig::new(LOCATOR)
        .domain_id(0)
        .node_name("isr_wake");
    let mut executor = Executor::open(&exec_config).expect("open the executor");

    // ---- phase 1: the executor's semaphore park ---------------------------
    static CALLBACKS: AtomicU32 = AtomicU32::new(0);
    let (_id, guard) = executor
        .register_guard_condition(|| {
            // Signed: a callback observed before its interrupt's stamp would
            // print negative, and the host test refuses that (issue 2748).
            let late_ns = now_ns() as i64 - FIRED_AT.load(Ordering::Acquire) as i64;
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            out!("ISR_WAKE {}", late_ns / 1000);
        })
        .expect("register the guard condition");
    GUARD.store(leak(guard), Ordering::Release);
    MODE.store(1, Ordering::Release);
    out!("ISR_WAKE_READY period_us={}", PERIOD_US);
    // SAFETY: `on_interrupt` touches only atomics and interrupt-safe calls.
    let rc = unsafe {
        nros_board_nuttx_isr_timer_start(PERIOD_US, on_interrupt, core::ptr::null_mut())
    };
    assert_eq!(rc, 0, "start the interrupt timer");
    while CALLBACKS.load(Ordering::Relaxed) < SAMPLES {
        let _ = executor.spin_once(core::time::Duration::from_millis(1000));
    }
    MODE.store(0, Ordering::Release);
    out!(
        "ISR_WAKE_DONE {} {} {}",
        FIRES.load(Ordering::Relaxed),
        CALLBACKS.load(Ordering::Relaxed),
        REFUSED.load(Ordering::Relaxed)
    );

    // ---- phase 2: the handle park ------------------------------------------
    // SAFETY: the port states the wake object's size and alignment; the
    // storage is leaked, so it outlives every signal.
    let w = unsafe {
        let layout = core::alloc::Layout::from_size_align(
            nros_platform_wake_storage_size(),
            nros_platform_wake_storage_align(),
        )
        .expect("wake layout");
        let p = alloc::alloc::alloc_zeroed(layout) as *mut core::ffi::c_void;
        assert!(!p.is_null(), "allocate the wake object");
        assert_eq!(nros_platform_wake_init(p), 0, "init the wake object");
        p
    };
    WAKE.store(w, Ordering::Release);
    REFUSED.store(0, Ordering::Relaxed);
    let mut woken = 0u32;
    for _ in 0..PARK_ROUNDS {
        // Drain anything a previous interrupt left, then let the next one in.
        // SAFETY: as above.
        while unsafe { nros_platform_wake_park_handles_us(w, core::ptr::null(), 0, 0) } == 0 {}
        let fires_before = FIRES.load(Ordering::Acquire);
        MODE.store(2, Ordering::Release);
        let parked_at = now_ns();
        // SAFETY: as above; no handles.
        let rc = unsafe { nros_platform_wake_park_handles_us(w, core::ptr::null(), 0, 1_000_000) };
        let parked_us = (now_ns() - parked_at) / 1000;
        MODE.store(0, Ordering::Release);
        // Interrupts that signalled during this park: a park that ends woken
        // with none was not woken by the interrupt (issue 2748).
        let fires = FIRES.load(Ordering::Acquire) - fires_before;
        let late_ns = now_ns() as i64 - FIRED_AT.load(Ordering::Acquire) as i64;
        if rc == 0 {
            woken += 1;
        }
        out!("ISR_PARK {} {} {} {}", late_ns / 1000, rc, parked_us, fires);
    }
    // SAFETY: stops the board's watchdog.
    unsafe { nros_board_nuttx_isr_timer_stop() };
    out!("ISR_PARK_DONE {} {}", woken, REFUSED.load(Ordering::Relaxed));
    // SAFETY: reads a counter.
    out!("ISR_WAKE_NOT_IN_ISR {}", unsafe {
        nros_board_nuttx_isr_timer_not_in_isr()
    });
    0
}

extern crate alloc;

/// Leak the guard handle for the interrupt to reach.
fn leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

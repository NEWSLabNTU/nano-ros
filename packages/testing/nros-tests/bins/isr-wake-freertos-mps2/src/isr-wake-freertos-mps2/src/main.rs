//! Issue 2696 -- a hardware interrupt wakes a parked executor through
//! `GuardCondition::trigger_from_isr`, on FreeRTOS + MPS2-AN385.
//!
//! CMSDK TIMER1 fires every [`PERIOD_US`]. Its handler calls
//! `trigger_from_isr` on a guard condition, which reaches FreeRTOS's
//! `xSemaphoreGiveFromISR` and a yield. The executor parks in `spin_once`
//! with a 1 s budget and nothing else to do, so only the interrupt can end
//! the park early. The guard's callback reads how long ago the interrupt
//! fired from TIMER1's own counter (it reloads at the interrupt), so the
//! measurement needs no clock that is safe to read from an interrupt.
//!
//! Each callback prints `ISR_WAKE <latency_us>`; after [`SAMPLES`] the image
//! prints `ISR_WAKE_DONE <fires> <callbacks> <refused>` and exits. The host test
//! asserts the bound. The thread `trigger()` called here instead trips
//! FreeRTOS's `configASSERT` (a critical section entered from an interrupt).
//!
//! Then a device's completion (phase 579 W3a). Each interrupt makes a
//! best-effort guard ready and then signals a device bound to a critical
//! sched context, through `Completion::signal_from_isr`. Both are ready when
//! the executor wakes, so the device's continuation must run first. Each
//! round prints nothing; after [`SAMPLES`] rounds the image prints
//! `DEVICE_PRIO_DONE <rounds> <device_first> <low_first> <refused>`.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use cortex_m_semihosting::debug::{EXIT_SUCCESS, exit};
use nros::{
    CompletionCell, Executor, ExecutorConfig, GuardCondition, Priority, SchedClass, SchedContext,
};
use nros_board_mps2_an385_freertos::{Config, Mps2An385, println};
use panic_semihosting as _;

extern crate nros_platform as _;

const CONFIG: &str = include_str!("../config.toml");

/// The interrupt's period.
const PERIOD_US: u32 = 20_000;
/// How many callbacks the run collects.
const SAMPLES: u32 = 50;

/// CMSDK APB TIMER1 (NVIC 9), clocked at the 25 MHz system clock.
const TIMER1_BASE: usize = 0x4000_1000;
const CTRL: usize = 0x00;
const VALUE: usize = 0x04;
const RELOAD: usize = 0x08;
const INTCLEAR: usize = 0x0C;
const CTRL_EN: u32 = 1 << 0;
const CTRL_INTEN: u32 = 1 << 3;
const TICKS_PER_US: u32 = 25;
const RELOAD_TICKS: u32 = PERIOD_US * TICKS_PER_US;
const TIMER1_IRQ: usize = 9;

/// The guard the interrupt triggers, leaked once set.
static GUARD: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// Interrupts taken.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose `trigger_from_isr` reported the wake reached nothing.
static REFUSED: AtomicU32 = AtomicU32::new(0);

/// The device phase is running: the interrupt drives the device, not `GUARD`.
static DEVICE_PHASE: AtomicBool = AtomicBool::new(false);
/// The best-effort guard the device phase readies first, leaked once set.
static LOW: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// The device's completion, as a port would hold it.
static DONE: CompletionCell = CompletionCell::new();
/// The low guard is ready and its callback has not run yet.
static LOW_PENDING: AtomicBool = AtomicBool::new(false);
static DEVICE_ROUNDS: AtomicU32 = AtomicU32::new(0);
static DEVICE_FIRST: AtomicU32 = AtomicU32::new(0);
static LOW_FIRST: AtomicU32 = AtomicU32::new(0);
static DEVICE_REFUSED: AtomicU32 = AtomicU32::new(0);

fn timer_write(off: usize, v: u32) {
    // SAFETY: TIMER1's registers, which nothing else on this board uses.
    unsafe { core::ptr::write_volatile((TIMER1_BASE + off) as *mut u32, v) };
}

fn timer_read(off: usize) -> u32 {
    // SAFETY: as `timer_write`.
    unsafe { core::ptr::read_volatile((TIMER1_BASE + off) as *const u32) }
}

/// TIMER1's handler: the board's vector table names it (a weak alias of
/// `Default_Handler` until defined here).
#[unsafe(no_mangle)]
extern "C" fn TIMER1_IRQHandler() {
    timer_write(INTCLEAR, 1);
    FIRES.fetch_add(1, Ordering::Relaxed);
    if DEVICE_PHASE.load(Ordering::Acquire) {
        // The lower-priority callback is ready first, then the device.
        let low = LOW.load(Ordering::Acquire);
        if !low.is_null() {
            LOW_PENDING.store(true, Ordering::Release);
            // SAFETY: a leaked handle, set before the phase began.
            let _ = unsafe { &*low }.trigger_from_isr();
        }
        if !DONE.signal_from_isr() {
            DEVICE_REFUSED.fetch_add(1, Ordering::Relaxed);
        }
        return;
    }
    let g = GUARD.load(Ordering::Acquire);
    if !g.is_null() {
        // SAFETY: a leaked handle, set before the interrupt was enabled.
        if !unsafe { &*g }.trigger_from_isr() {
            REFUSED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn start_timer() {
    // NVIC: priority 6 (of 8), below `configMAX_SYSCALL_INTERRUPT_PRIORITY`'s
    // 5 as FreeRTOS requires of a handler that calls a `...FromISR` API; then
    // enable. IPR is byte-addressed, top 3 bits significant.
    // SAFETY: the NVIC's priority and set-enable registers, single core.
    unsafe {
        core::ptr::write_volatile((0xE000_E400 + TIMER1_IRQ) as *mut u8, 6 << 5);
        core::ptr::write_volatile(0xE000_E100 as *mut u32, 1 << TIMER1_IRQ);
    }
    timer_write(CTRL, 0);
    timer_write(RELOAD, RELOAD_TICKS);
    timer_write(VALUE, RELOAD_TICKS);
    timer_write(INTCLEAR, 1);
    timer_write(CTRL, CTRL_EN | CTRL_INTEN);
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(Config::from_toml(CONFIG), |config| {
        nros_rmw_zenoh::register().expect("register the zenoh backend");
        let exec_config = ExecutorConfig::new(config.base.zenoh_locator)
            .domain_id(config.base.domain_id)
            .node_name("isr_wake");
        let mut executor = open_executor(&exec_config)?;

        static CALLBACKS: AtomicU32 = AtomicU32::new(0);
        let (_id, guard) = executor.register_guard_condition(|| {
            // Ticks since the interrupt reloaded TIMER1: how late this
            // callback ran.
            let elapsed = RELOAD_TICKS.saturating_sub(timer_read(VALUE));
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            println!("ISR_WAKE {}", elapsed / TICKS_PER_US);
        })?;
        GUARD.store(alloc_leak(guard), Ordering::Release);

        // The device phase's two sources, registered before the first spin.
        let (h_low, low) = executor.register_guard_condition(|| {
            LOW_PENDING.store(false, Ordering::Release);
        })?;
        let (h_dev, done) = executor.register_device(|| {
            if LOW_PENDING.load(Ordering::Acquire) {
                DEVICE_FIRST.fetch_add(1, Ordering::Relaxed);
            } else {
                LOW_FIRST.fetch_add(1, Ordering::Relaxed);
            }
            DEVICE_ROUNDS.fetch_add(1, Ordering::Relaxed);
        })?;
        let sc_low = executor.create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            priority: Priority::BestEffort,
            ..Default::default()
        })?;
        let sc_dev = executor.create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            priority: Priority::Critical,
            ..Default::default()
        })?;
        executor.bind_handle_to_sched_context(h_low, sc_low)?;
        executor.bind_handle_to_sched_context(h_dev, sc_dev)?;
        LOW.store(alloc_leak(low), Ordering::Release);
        DONE.set(done);

        println!("ISR_WAKE_READY period_us={}", PERIOD_US);
        start_timer();

        while CALLBACKS.load(Ordering::Relaxed) < SAMPLES {
            let _ = executor.spin_once(core::time::Duration::from_millis(1000));
        }
        println!(
            "ISR_WAKE_DONE {} {} {}",
            FIRES.load(Ordering::Relaxed),
            CALLBACKS.load(Ordering::Relaxed),
            REFUSED.load(Ordering::Relaxed)
        );

        DEVICE_PHASE.store(true, Ordering::Release);
        while DEVICE_ROUNDS.load(Ordering::Relaxed) < SAMPLES {
            let _ = executor.spin_once(core::time::Duration::from_millis(1000));
        }
        println!(
            "DEVICE_PRIO_DONE {} {} {} {}",
            DEVICE_ROUNDS.load(Ordering::Relaxed),
            DEVICE_FIRST.load(Ordering::Relaxed),
            LOW_FIRST.load(Ordering::Relaxed),
            DEVICE_REFUSED.load(Ordering::Relaxed)
        );
        exit(EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), nros::NodeError>(())
    });
    unreachable!()
}

/// `Executor::open`, out of line: the session it opens and the tables it lays
/// out are this function's temporaries, not the run closure's, whose frame
/// holds the executor for the whole run. Inlined, they put the closure's frame
/// at 10 KB; apart, the two frames together are smaller than that one was.
#[inline(never)]
fn open_executor(config: &ExecutorConfig<'_>) -> Result<Executor<'static>, nros::NodeError> {
    Executor::open(config)
}

/// Leak the guard handle for the interrupt to reach.
fn alloc_leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    extern crate alloc;
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

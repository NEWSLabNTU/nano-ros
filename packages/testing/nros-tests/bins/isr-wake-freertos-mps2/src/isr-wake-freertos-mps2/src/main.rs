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

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use cortex_m_semihosting::debug::{EXIT_SUCCESS, exit};
use nros::{Executor, ExecutorConfig, GuardCondition};
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
        let mut executor = Executor::open(&exec_config)?;

        static CALLBACKS: AtomicU32 = AtomicU32::new(0);
        let (_id, guard) = executor.register_guard_condition(|| {
            // Ticks since the interrupt reloaded TIMER1: how late this
            // callback ran.
            let elapsed = RELOAD_TICKS.saturating_sub(timer_read(VALUE));
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            println!("ISR_WAKE {}", elapsed / TICKS_PER_US);
        })?;
        GUARD.store(alloc_leak(guard), Ordering::Release);
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
        exit(EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), nros::NodeError>(())
    });
    unreachable!()
}

/// Leak the guard handle for the interrupt to reach.
fn alloc_leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    extern crate alloc;
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

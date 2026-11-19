//! Issue 2706 -- a hardware interrupt wakes a parked executor through
//! `GuardCondition::trigger_from_isr`, on bare-metal MPS2-AN385.
//!
//! The bare-metal twin of `isr-wake-freertos-mps2`. CMSDK TIMER1 fires every
//! [`PERIOD_US`] through the platform's periodic slot
//! (`nros_platform_mps2_an385::sporadic_timer`), whose handler runs
//! [`on_timer`]: `trigger_from_isr` on a guard condition, which reaches the
//! port's wake flag. The executor parks in `spin_once` with a 1 s budget and
//! nothing else to do, in the board's tickless `wfi` idle, so only the
//! interrupt can end the park early. The guard's callback reads how long ago
//! the interrupt fired from TIMER1's own counter (it reloads at the
//! interrupt), so the measurement needs no clock that is safe to read from an
//! interrupt.
//!
//! Each callback prints `ISR_WAKE <latency_us>`; after [`SAMPLES`] the image
//! prints `ISR_WAKE_DONE <fires> <callbacks> <refused>` and exits. The host
//! test asserts the bound.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use nros::{Executor, ExecutorConfig, GuardCondition};
use nros_board_mps2_an385::{Config, println, run_bare};
use panic_semihosting as _;

/// The interrupt's period.
const PERIOD_US: u32 = 20_000;
/// How many callbacks the run collects.
const SAMPLES: u32 = 50;

/// CMSDK APB TIMER1's current-value register, clocked at the 25 MHz system
/// clock. The platform's periodic slot owns the timer; this image only reads
/// it.
const TIMER1_VALUE: usize = 0x4000_1000 + 0x04;
const TICKS_PER_US: u32 = 25;
const RELOAD_TICKS: u32 = PERIOD_US * TICKS_PER_US;

/// The guard the interrupt triggers, leaked once set.
static GUARD: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// Interrupts taken.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose `trigger_from_isr` reported the wake reached nothing.
static REFUSED: AtomicU32 = AtomicU32::new(0);

fn timer_value() -> u32 {
    // SAFETY: TIMER1's value register, read-only here.
    unsafe { core::ptr::read_volatile(TIMER1_VALUE as *const u32) }
}

/// Runs in TIMER1's interrupt handler (the platform clears the latch first).
extern "C" fn on_timer(_ctx: *mut core::ffi::c_void) {
    FIRES.fetch_add(1, Ordering::Relaxed);
    let g = GUARD.load(Ordering::Acquire);
    if !g.is_null() {
        // SAFETY: a leaked handle, set before the timer was started.
        if !unsafe { &*g }.trigger_from_isr() {
            REFUSED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    // `nros_tests::alloc::BAREMETAL_ISR_WAKE_PORT`, as in `system.toml`.
    let config = Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
        ip: [10, 0, 2, 10],
        prefix: 24,
        gateway: [10, 0, 2, 2],
        zenoh_locator: "tcp/10.0.2.2:10560",
        domain_id: 0,
    };
    run_bare(config, |config| {
        nros_rmw_zenoh::register().expect("register the zenoh backend");
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("isr_wake");
        let mut executor = Executor::open(&exec_config)?;

        static CALLBACKS: AtomicU32 = AtomicU32::new(0);
        let (_id, guard) = executor.register_guard_condition(|| {
            // Ticks since the interrupt reloaded TIMER1: how late this
            // callback ran.
            let elapsed = RELOAD_TICKS.saturating_sub(timer_value());
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            println!("ISR_WAKE {}", elapsed / TICKS_PER_US);
        })?;
        GUARD.store(alloc_leak(guard), Ordering::Release);
        println!("ISR_WAKE_READY period_us={}", PERIOD_US);
        nros_platform_mps2_an385::sporadic_timer::register_periodic(
            PERIOD_US,
            on_timer,
            core::ptr::null_mut(),
        )
        .expect("start TIMER1");

        while CALLBACKS.load(Ordering::Relaxed) < SAMPLES {
            let _ = executor.spin_once(core::time::Duration::from_millis(1000));
        }
        nros_platform_mps2_an385::sporadic_timer::destroy();
        println!(
            "ISR_WAKE_DONE {} {} {}",
            FIRES.load(Ordering::Relaxed),
            CALLBACKS.load(Ordering::Relaxed),
            REFUSED.load(Ordering::Relaxed)
        );
        Ok::<(), nros::NodeError>(())
    })
}

/// Leak the guard handle for the interrupt to reach.
fn alloc_leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    extern crate alloc;
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

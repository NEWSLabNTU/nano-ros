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
//! prints `ISR_WAKE_DONE <fires> <callbacks> <refused>`.
//!
//! Then the race probe, which makes the park's one window deterministic
//! instead of a matter of timing. TIMER1 is stopped, so nothing else can end
//! a park. For each of [`RACE_ROUNDS`] rounds the platform's park hook
//! (`wake::set_park_probe`, feature `park-race-probe`) pends DUALTIMER
//! exactly between the park's flag read and its idle, and DUALTIMER's handler
//! calls `trigger_from_isr`. With the read inside the idle's critical
//! section the pending interrupt ends the `wfi` at once. With the read
//! outside it, the handler runs before the idle, and the park sleeps to its
//! deadline. Each round prints `ISR_RACE <latency_us>`, measured from the
//! pend, then `ISR_RACE_DONE <rounds>`, and the image exits. The host test
//! asserts both bounds.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use nros::{Executor, ExecutorConfig, GuardCondition};
use nros_board_mps2_an385::{Config, println, run_bare};
use panic_semihosting as _;

/// The interrupt's period.
const PERIOD_US: u32 = 20_000;
/// How many callbacks the run collects.
const SAMPLES: u32 = 50;
/// How many race-probe rounds the run makes.
const RACE_ROUNDS: u32 = 10;

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
/// The race probe pends DUALTIMER on the next park when set.
static RACE_ARMED: AtomicBool = AtomicBool::new(false);
/// When the probe pended it, in microseconds of `clock_ns` (wrapping, never
/// 0); 0 in the timer phase. 32 bits: thumbv7m has no 64-bit atomics.
static PENDED_AT_US: AtomicU32 = AtomicU32::new(0);

fn now_us() -> u32 {
    (nros_platform_mps2_an385::clock::clock_ns() / 1000) as u32
}

/// The park hook: in the window between the flag read and the idle, pend
/// DUALTIMER once per armed round.
fn race_probe() {
    if RACE_ARMED.swap(false, Ordering::AcqRel) {
        PENDED_AT_US.store(now_us().max(1), Ordering::Release);
        cortex_m::peripheral::NVIC::pend(mps2_an385_pac::Interrupt::DUALTIMER);
    }
}

/// DUALTIMER's handler (NVIC 10; the PAC's vector table names it): the
/// probe's interrupt, which triggers the guard as TIMER1's does.
#[unsafe(no_mangle)]
extern "C" fn DUALTIMER() {
    on_timer(core::ptr::null_mut());
}

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
        static RACE_CALLBACKS: AtomicU32 = AtomicU32::new(0);
        let (_id, guard) = executor.register_guard_condition(|| {
            let pended = PENDED_AT_US.swap(0, Ordering::AcqRel);
            if pended != 0 {
                let late_us = now_us().wrapping_sub(pended);
                RACE_CALLBACKS.fetch_add(1, Ordering::Relaxed);
                println!("ISR_RACE {}", late_us);
                return;
            }
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

        // The race probe. Drain anything the timer phase left, then one pend
        // per round, in the park's window.
        for _ in 0..3 {
            let _ = executor.spin_once(core::time::Duration::from_millis(10));
        }
        nros_platform_mps2_an385::wake::set_park_probe(race_probe);
        // SAFETY: DUALTIMER's handler is defined above and touches only
        // atomics; single core.
        unsafe { cortex_m::peripheral::NVIC::unmask(mps2_an385_pac::Interrupt::DUALTIMER) };
        for round in 0..RACE_ROUNDS {
            RACE_ARMED.store(true, Ordering::Release);
            // One spin: its park pends the interrupt. A lost wake makes this
            // spin run to its budget, and the callback then reports it.
            let mut spins = 0;
            while RACE_CALLBACKS.load(Ordering::Relaxed) <= round && spins < 5 {
                let _ = executor.spin_once(core::time::Duration::from_millis(1000));
                spins += 1;
            }
        }
        println!("ISR_RACE_DONE {}", RACE_CALLBACKS.load(Ordering::Relaxed));
        Ok::<(), nros::NodeError>(())
    })
}

/// Leak the guard handle for the interrupt to reach.
fn alloc_leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    extern crate alloc;
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

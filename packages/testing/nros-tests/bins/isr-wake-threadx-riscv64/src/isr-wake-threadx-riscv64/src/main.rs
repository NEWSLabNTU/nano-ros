//! Issue 2706 -- a hardware interrupt wakes a parked executor through
//! `GuardCondition::trigger_from_isr`, on ThreadX on the QEMU RISC-V `virt`
//! machine.
//!
//! The ThreadX twin of `isr-wake-freertos-mps2`. The interrupt is the
//! machine timer (CLINT `mtimecmp`), which drives ThreadX's 100 Hz tick: the
//! board's trap handler runs `_tx_timer_interrupt` and then the image's
//! [`nros_rv_virt_tick_hook`], still in the trap, with ThreadX's interrupt
//! context saved. Every [`TICKS_PER_FIRE`] ticks the hook stamps `mtime` and
//! calls `trigger_from_isr` on a guard condition, which reaches the port's
//! `tx_semaphore_ceiling_put`. The executor parks in `spin_once` with a 1 s
//! budget and nothing else to do, so only the interrupt can end the park
//! early. The guard's callback reads how long ago the interrupt fired from
//! `mtime`.
//!
//! Each callback prints `ISR_WAKE <latency_us>`; after [`SAMPLES`] the image
//! prints `ISR_WAKE_DONE <fires> <callbacks> <refused>` and exits.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use nros::{Executor, ExecutorConfig, GuardCondition};
use nros_board_threadx_qemu_riscv64::{Config, ThreadxQemuRiscv64, println};

nros::panic_to_platform!();

// Network config lives in a sibling `config.toml`, compile-baked here.
const CONFIG: &str = include_str!("../config.toml");

/// ThreadX ticks at 100 Hz, so the interrupt fires every 20 ms.
const TICKS_PER_FIRE: u32 = 2;
/// How many callbacks the run collects.
const SAMPLES: u32 = 50;
/// The CLINT `mtime` counter runs at 10 MHz.
const MTIME_TICKS_PER_US: u64 = 10;

/// The guard the interrupt triggers, leaked once set.
static GUARD: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// The hook acts only once the guard is set and the run has started.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Ticks seen since arming.
static TICKS: AtomicU32 = AtomicU32::new(0);
/// Interrupts that triggered the guard.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose `trigger_from_isr` reported the wake reached nothing.
static REFUSED: AtomicU32 = AtomicU32::new(0);
/// `mtime` when the last trigger fired.
static FIRED_AT: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    /// The board's free-running CLINT `mtime` (10 MHz).
    fn hwtimer_now() -> u64;
}

fn mtime() -> u64 {
    // SAFETY: reads the CLINT counter; no state.
    unsafe { hwtimer_now() }
}

/// The board's tick hook (a weak no-op in its `trap.c`, defined here): runs in
/// the machine-timer trap, after `_tx_timer_interrupt`.
#[unsafe(no_mangle)]
extern "C" fn nros_rv_virt_tick_hook() {
    if !ARMED.load(Ordering::Acquire) {
        return;
    }
    if TICKS.fetch_add(1, Ordering::Relaxed) % TICKS_PER_FIRE != TICKS_PER_FIRE - 1 {
        return;
    }
    let g = GUARD.load(Ordering::Acquire);
    if g.is_null() {
        return;
    }
    FIRES.fetch_add(1, Ordering::Relaxed);
    FIRED_AT.store(mtime(), Ordering::Release);
    // SAFETY: a leaked handle, set before the hook was armed.
    if !unsafe { &*g }.trigger_from_isr() {
        REFUSED.fetch_add(1, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = ThreadxQemuRiscv64::run_bare(Config::from_toml(CONFIG), || {
        nros_rmw_zenoh::register().expect("register the zenoh backend");
        let config = Config::from_toml(CONFIG);
        let exec_config = ExecutorConfig::new(config.base.zenoh_locator)
            .domain_id(config.base.domain_id)
            .node_name("isr_wake");
        let mut executor = Executor::open(&exec_config)?;

        static CALLBACKS: AtomicU32 = AtomicU32::new(0);
        let (_id, guard) = executor.register_guard_condition(|| {
            let late = mtime().wrapping_sub(FIRED_AT.load(Ordering::Acquire));
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            println!("ISR_WAKE {}", late / MTIME_TICKS_PER_US);
        })?;
        GUARD.store(alloc_leak(guard), Ordering::Release);
        println!("ISR_WAKE_READY period_us={}", TICKS_PER_FIRE * 10_000);
        ARMED.store(true, Ordering::Release);

        while CALLBACKS.load(Ordering::Relaxed) < SAMPLES {
            let _ = executor.spin_once(core::time::Duration::from_millis(1000));
        }
        ARMED.store(false, Ordering::Release);
        println!(
            "ISR_WAKE_DONE {} {} {}",
            FIRES.load(Ordering::Relaxed),
            CALLBACKS.load(Ordering::Relaxed),
            REFUSED.load(Ordering::Relaxed)
        );
        Ok::<(), nros::NodeError>(())
    });
    // `run_bare` diverges internally (kernel entry, then exit_success/failure).
    unreachable!()
}

/// Leak the guard handle for the interrupt to reach.
fn alloc_leak(g: GuardCondition<'static>) -> *mut GuardCondition<'static> {
    extern crate alloc;
    alloc::boxed::Box::leak(alloc::boxed::Box::new(g))
}

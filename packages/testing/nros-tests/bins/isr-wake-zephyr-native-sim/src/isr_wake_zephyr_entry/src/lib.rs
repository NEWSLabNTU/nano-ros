//! Issue 2706 -- a Zephyr interrupt wakes a parked executor through
//! `GuardCondition::trigger_from_isr`, on native_sim.
//!
//! The Zephyr twin of `isr-wake-freertos-mps2`. A `k_timer` fires every
//! [`PERIOD_US`]; its expiry function runs in the system clock's interrupt
//! handler (`src/isr_timer.c`, which counts any expiry that is not), stamps the
//! cycle counter and calls [`nros_isr_test_on_interrupt`], which calls
//! `trigger_from_isr` on a guard condition. That reaches the port's
//! `nros_platform_wake_signal_from_isr` (`k_sem_give`, and an eventfd write
//! once a handle park has made one). The executor parks in `spin_once` with a
//! 1 s budget and nothing else to do, so only the interrupt can end the park
//! early. The guard's callback reads how long ago the expiry ran.
//!
//! Each callback prints `ISR_WAKE <latency_us>`; after [`SAMPLES`] the image
//! prints `ISR_WAKE_DONE <fires> <callbacks> <refused>` and
//! `ISR_WAKE_NOT_IN_ISR <n>`. The host test asserts the bound.

#![no_std]

extern crate alloc;
extern crate zephyr;

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use nros::{Executor, ExecutorConfig, GuardCondition};

nros::force_link_backend!(nros_rmw_zenoh);

/// The interrupt's period.
const PERIOD_US: u32 = 20_000;
/// How many callbacks the run collects.
const SAMPLES: u32 = 50;

unsafe extern "C" {
    fn nros_isr_test_start(period_us: u32);
    fn nros_isr_test_stop();
    fn nros_isr_test_elapsed_us() -> u32;
    fn nros_isr_test_not_in_isr() -> u32;
    fn nros_runtime_locator_override() -> *const core::ffi::c_char;
}

/// The guard the interrupt triggers, leaked once set.
static GUARD: AtomicPtr<GuardCondition<'static>> = AtomicPtr::new(core::ptr::null_mut());
/// Interrupts taken.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose `trigger_from_isr` reported the wake reached nothing.
static REFUSED: AtomicU32 = AtomicU32::new(0);
/// Guard callbacks run.
static CALLBACKS: AtomicU32 = AtomicU32::new(0);

/// Called from the timer's expiry, in interrupt context.
#[unsafe(no_mangle)]
pub extern "C" fn nros_isr_test_on_interrupt() {
    FIRES.fetch_add(1, Ordering::Relaxed);
    let g = GUARD.load(Ordering::Acquire);
    if !g.is_null() {
        // SAFETY: a leaked handle, set before the timer was started.
        if !unsafe { &*g }.trigger_from_isr() {
            REFUSED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() {
    nros_platform::log::init_default();
    let _ = nros_platform::zephyr::wait_network(2000);
    nros_rmw_zenoh::register().expect("register the zenoh backend");

    // The harness's per-test router (`-testargs --nros-locator=<loc>`), else
    // the build's baked locator.
    // SAFETY: argv-backed, process lifetime, or NULL.
    let p = unsafe { nros_runtime_locator_override() };
    let locator = if p.is_null() {
        "tcp/127.0.0.1:7447"
    } else {
        // SAFETY: a NUL-terminated string the platform owns for the process.
        unsafe { core::ffi::CStr::from_ptr(p) }
            .to_str()
            .unwrap_or("tcp/127.0.0.1:7447")
    };
    let config = ExecutorConfig::new(locator).node_name("isr_wake");
    let mut executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(e) => {
            zephyr::printkln!("ISR_WAKE_OPEN_FAILED {:?}", e);
            return;
        }
    };
    let (_id, guard) = executor
        .register_guard_condition(|| {
            // SAFETY: reads the C helper's stamp; main-thread call.
            let late = unsafe { nros_isr_test_elapsed_us() };
            CALLBACKS.fetch_add(1, Ordering::Relaxed);
            zephyr::printkln!("ISR_WAKE {}", late);
        })
        .expect("register the guard condition");
    let leaked: &'static mut GuardCondition<'static> = alloc::boxed::Box::leak(alloc::boxed::Box::new(guard));
    GUARD.store(leaked, Ordering::Release);
    zephyr::printkln!("ISR_WAKE_READY period_us={}", PERIOD_US);
    // SAFETY: the C helper's timer, started once.
    unsafe { nros_isr_test_start(PERIOD_US) };

    while CALLBACKS.load(Ordering::Relaxed) < SAMPLES {
        let _ = executor.spin_once(core::time::Duration::from_millis(1000));
    }
    // SAFETY: as above.
    unsafe { nros_isr_test_stop() };
    zephyr::printkln!(
        "ISR_WAKE_DONE {} {} {}",
        FIRES.load(Ordering::Relaxed),
        CALLBACKS.load(Ordering::Relaxed),
        REFUSED.load(Ordering::Relaxed)
    );
    // SAFETY: a counter read.
    zephyr::printkln!("ISR_WAKE_NOT_IN_ISR {}", unsafe { nros_isr_test_not_in_isr() });
}

//! nros platform implementation for FreeRTOS.
//!
//! Calls FreeRTOS C APIs (`xTaskCreate`, `pvPortMalloc`, `xSemaphoreCreateRecursiveMutex`,
//! etc.) via `extern "C"` declarations resolved at link time from the FreeRTOS
//! kernel library linked by the board crate.
//!
//! # Threading model
//!
//! FreeRTOS uses:
//! - Tasks: `xTaskCreate` / `vTaskDelete`
//! - Mutexes: recursive semaphores (`xSemaphoreCreateRecursiveMutex`)
//! - Condition variables: manual implementation using counting semaphore + mutex
//! - Task join: event groups (`xEventGroupWaitBits`)

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use nros_platform_api::{PlatformAlloc, PlatformClock};

mod ffi;
#[cfg(feature = "lwip")]
pub mod net;
mod types;

/// Zero-sized type implementing all platform methods for FreeRTOS.
pub struct FreeRtosPlatform;

// ============================================================================
// Phase 97.1.cs / 100.1 — `critical_section::Impl` (Cortex-M or Cortex-R)
// ============================================================================
//
// dust-dds's oneshot channels reference `_critical_section_1_0_acquire` /
// `_release` symbols; the example crate links against this impl so the
// symbols resolve at link time. Cortex-M and Cortex-R provide
// equivalent "globally mask interrupts" mechanisms; we save the prior
// state into the restore token so nested critical sections nest cleanly.
//
// The two paths are mutually exclusive — the `critical-section` (M) and
// `cortex-r` (R) features each register one global `set_impl!`, so
// enabling both would double-define the static.

#[cfg(all(feature = "critical-section", feature = "cortex-r"))]
compile_error!(
    "nros-platform-freertos: features `critical-section` (Cortex-M PRIMASK) and \
     `cortex-r` (ARMv7-R CPSR I-bit) are mutually exclusive — pick one"
);

#[cfg(feature = "critical-section")]
mod cs_impl {
    use cortex_m::register::primask;

    struct FreeRtosCs;
    critical_section::set_impl!(FreeRtosCs);

    unsafe impl critical_section::Impl for FreeRtosCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            // Read prior PRIMASK (bit 0 = 1 means interrupts already
            // disabled), then disable interrupts. The token encodes
            // the prior state so `release` can re-enable only if we
            // were the outermost acquire.
            let was_enabled = primask::read().is_active();
            cortex_m::interrupt::disable();
            // RawRestoreState = u32 (matches the `restore-state-u32`
            // critical-section feature).
            if was_enabled { 1 } else { 0 }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            if token == 1 {
                // SAFETY: prior state was "enabled"; we're at the
                // outermost acquire.
                unsafe { cortex_m::interrupt::enable() };
            }
        }
    }
}

// Phase 100.1 — ARMv7-R CPSR I-bit critical section.
//
// On Cortex-R the global interrupt mask lives in CPSR bit 7 (I-bit).
// `cpsid i` sets it, `cpsie i` clears it. We snapshot the prior value
// via `mrs Rd, cpsr` and stash it in the restore token, so nested
// `acquire`/`release` pairs unwind correctly: only the outermost
// `release` re-enables interrupts.
//
// `armv7r-none-eabihf` doesn't ship a `cortex-r` analogue of the
// `cortex-m` crate; raw inline asm is the standard way (the same
// pattern used by the FreeRTOS ARM_CR5 port and by RTIC's R-profile
// support).
#[cfg(feature = "cortex-r")]
mod cs_impl {
    use core::arch::asm;

    struct FreeRtosCs;
    critical_section::set_impl!(FreeRtosCs);

    unsafe impl critical_section::Impl for FreeRtosCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            let cpsr: u32;
            // Read CPSR, then mask IRQs (bit 7). Mask FIQs (bit 6) is
            // intentionally left alone — FIQ on the Orin SPE carries
            // the high-rate timer tick and should not be deferred.
            unsafe {
                asm!(
                    "mrs {0}, cpsr",
                    "cpsid i",
                    out(reg) cpsr,
                    options(nomem, nostack, preserves_flags),
                );
            }
            // Bit 7 of the snapshot: 0 = IRQs were enabled, 1 = already
            // masked. Token format matches the M-side convention: 1
            // means "we did the masking, restore on release".
            if (cpsr & (1 << 7)) == 0 { 1 } else { 0 }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            if token == 1 {
                // SAFETY: prior state was "enabled"; we're at the
                // outermost acquire.
                unsafe {
                    asm!("cpsie i", options(nomem, nostack, preserves_flags));
                }
            }
        }
    }
}

// ============================================================================
// Clock — xTaskGetTickCount
// ============================================================================

impl nros_platform_api::PlatformClock for FreeRtosPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        let ticks = unsafe { ffi::xTaskGetTickCount() };
        // portTICK_PERIOD_MS is typically 1 for configTICK_RATE_HZ=1000.
        // We assume 1ms ticks; board crates can override if different.
        ticks as u64
    }

    #[inline]
    fn clock_us() -> u64 {
        <Self as PlatformClock>::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — pvPortMalloc / vPortFree
// ============================================================================

impl nros_platform_api::PlatformAlloc for FreeRtosPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        unsafe { ffi::pvPortMalloc(size) }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        use nros_platform_api::PlatformAlloc;
        // FreeRTOS has no realloc — allocate new, copy, free old.
        if ptr.is_null() {
            return <Self as PlatformAlloc>::alloc(size);
        }
        let new_ptr = <Self as PlatformAlloc>::alloc(size);
        if !new_ptr.is_null() {
            // We don't know the old size, so copy `size` bytes (caller's
            // responsibility to ensure old allocation >= size).
            unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size) };
            <Self as PlatformAlloc>::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        unsafe { ffi::vPortFree(ptr) }
    }
}

// ============================================================================
// Sleep — vTaskDelay
// ============================================================================

impl nros_platform_api::PlatformSleep for FreeRtosPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        unsafe { ffi::vTaskDelay(ms as u32) };
    }

    #[inline]
    fn sleep_s(s: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Yield — 1-tick vTaskDelay (approximation)
// ============================================================================

impl nros_platform_api::PlatformYield for FreeRtosPlatform {
    #[inline]
    fn yield_now() {
        // FreeRTOS's true cooperative yield is the C macro `taskYIELD()`
        // (which expands to `portYIELD()` — a port-specific inline asm
        // that triggers PendSV on Cortex-M). Calling a macro from Rust
        // FFI would need a one-line C shim compiled against FreeRTOS
        // headers (deferred — see Phase 77.22 note).
        //
        // `vTaskDelay(1)` blocks for one scheduler tick, which is
        // effectively a yield: the scheduler picks the highest-priority
        // ready task; if only the caller is runnable it resumes after
        // the tick. `vTaskDelay(0)` is a documented no-op in FreeRTOS,
        // so we pass 1.
        unsafe { ffi::vTaskDelay(1) };
    }
}

// ============================================================================
// Phase 110.E.b — PlatformTimer (FreeRTOS xTimerCreate)
// ============================================================================
//
// FreeRTOS callback signature is `void(TimerHandle_t)` — single
// timer-handle arg, no user_data slot. We pack the user callback +
// data via the timer ID slot:
//   id = Box::leak(Box::new(BridgeData { user_callback, user_data }))
// then the static thunk reads `pvTimerGetTimerID(timer)` to recover
// the bridge struct and dispatch.
//
// FreeRTOS ticks: period_us / portTICK_PERIOD_MS — we assume 1 ms
// tick (default `configTICK_RATE_HZ = 1000`). Sub-ms periods round
// up to 1 tick.

#[allow(dead_code)] // Phase 110.E.b — keeps the bridge alive across timer lifetime.
struct FreeRtosTimerBridge {
    user_callback: extern "C" fn(*mut c_void),
    user_data: *mut c_void,
}

unsafe impl Send for FreeRtosTimerBridge {}
unsafe impl Sync for FreeRtosTimerBridge {}

extern "C" fn freertos_timer_thunk(timer: *mut c_void) {
    // SAFETY: `xTimerCreate` was passed a leaked Box pointer as
    // `timer_id`; we recover it via `pvTimerGetTimerID`. The bridge
    // outlives the timer because `destroy` joins before freeing.
    let bridge_ptr = unsafe { ffi::pvTimerGetTimerID(timer) } as *const FreeRtosTimerBridge;
    if bridge_ptr.is_null() {
        return;
    }
    let bridge = unsafe { &*bridge_ptr };
    (bridge.user_callback)(bridge.user_data);
}

/// FreeRTOS timer handle — packs the native `TimerHandle_t` plus the
/// leaked bridge box so destroy can free both atomically.
pub struct FreeRtosTimerHandle {
    timer: *mut c_void,
    bridge: *mut FreeRtosTimerBridge,
}

unsafe impl Send for FreeRtosTimerHandle {}
unsafe impl Sync for FreeRtosTimerHandle {}

impl core::fmt::Debug for FreeRtosTimerHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FreeRtosTimerHandle").finish_non_exhaustive()
    }
}

impl nros_platform_api::PlatformTimer for FreeRtosPlatform {
    type TimerHandle = FreeRtosTimerHandle;

    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        use nros_platform_api::TimerError;
        if period_us == 0 {
            return Err(TimerError::OutOfRange);
        }
        // Assume 1 ms tick (default `configTICK_RATE_HZ = 1000`);
        // sub-ms periods round up to 1 tick.
        let period_ticks = ((period_us + 999) / 1000).max(1);

        // Box the bridge + leak. `destroy` re-acquires via Box::from_raw.
        // SAFETY: requires `feature = "alloc"`; FreeRTOS platform
        // is std-incompatible but always has `alloc` (heapless via
        // `pvPortMalloc`).
        extern crate alloc;
        let bridge = alloc::boxed::Box::new(FreeRtosTimerBridge {
            user_callback: callback,
            user_data,
        });
        let bridge_ptr = alloc::boxed::Box::into_raw(bridge);

        let timer = unsafe {
            ffi::xTimerCreate(
                b"nros_sporadic\0".as_ptr() as *const _,
                period_ticks,
                1, // pdTRUE — auto-reload
                bridge_ptr as *mut c_void,
                freertos_timer_thunk,
            )
        };
        if timer.is_null() {
            // Recover the leaked bridge.
            unsafe { drop(alloc::boxed::Box::from_raw(bridge_ptr)) };
            return Err(TimerError::KernelError);
        }
        let started = unsafe { ffi::xTimerStart(timer, 0) };
        if started == 0 {
            unsafe {
                ffi::xTimerDelete(timer, 0);
                drop(alloc::boxed::Box::from_raw(bridge_ptr));
            }
            return Err(TimerError::KernelError);
        }
        Ok(FreeRtosTimerHandle {
            timer,
            bridge: bridge_ptr,
        })
    }

    fn destroy(handle: Self::TimerHandle) {
        // SAFETY: timer + bridge are owned by `handle`; deleting the
        // FreeRTOS timer drains in-flight callbacks (kernel guarantees
        // delete blocks until the timer-service-task finishes any
        // pending invocation), so the bridge box is safe to drop after.
        unsafe {
            ffi::xTimerDelete(handle.timer, 0);
            extern crate alloc;
            drop(alloc::boxed::Box::from_raw(handle.bridge));
        }
    }
}

// ============================================================================
// Phase 110.D — PlatformScheduler (FreeRTOS)
// ============================================================================
//
// FreeRTOS priorities run high-numeric = high-priority (same direction
// as POSIX), so the `os_pri` field maps directly to
// `vTaskPrioritySet`'s `new_priority` argument. SCHED_RR has no
// FreeRTOS analog — every task is preemptive-priority by default;
// configUSE_TIME_SLICING (round-robin among same-priority tasks) is a
// global build-time knob, not per-task. We accept `RoundRobin` and
// just set the priority, treating `quantum_ms` as advisory.
//
// `Deadline` (Linux SCHED_DEADLINE) and `Sporadic` (NuttX
// SCHED_SPORADIC) have no FreeRTOS analog; both surface
// `Unsupported`. Affinity needs FreeRTOS V11+'s
// `vTaskCoreAffinitySet`; on single-core ports the affinity surface
// stays `Unsupported`.

impl nros_platform_api::PlatformScheduler for FreeRtosPlatform {
    fn set_current_thread_policy(
        p: nros_platform_api::SchedPolicy,
    ) -> Result<(), nros_platform_api::SchedError> {
        use nros_platform_api::{SchedError, SchedPolicy};
        let new_priority = match p {
            SchedPolicy::Fifo { os_pri } | SchedPolicy::RoundRobin { os_pri, .. } => {
                os_pri as u32
            }
            SchedPolicy::Deadline { .. } | SchedPolicy::Sporadic { .. } => {
                return Err(SchedError::Unsupported);
            }
        };
        // SAFETY: `xTaskGetCurrentTaskHandle` returns the calling
        // task's TCB pointer; `vTaskPrioritySet(NULL, ...)` would do
        // the same but the explicit form is clearer.
        unsafe {
            let task = ffi::xTaskGetCurrentTaskHandle();
            ffi::vTaskPrioritySet(task, new_priority);
        }
        Ok(())
    }

    #[inline]
    fn yield_now() {
        // Reuse the same `vTaskDelay(1)` trick as `PlatformYield`.
        unsafe { ffi::vTaskDelay(1) };
    }

    fn set_affinity(_cpu_mask: u32) -> Result<(), nros_platform_api::SchedError> {
        // FreeRTOS V11 introduces `vTaskCoreAffinitySet` for SMP
        // ports. Single-core builds (the QEMU MPS2-AN385 default)
        // have no affinity API; surface Unsupported until the SMP
        // bring-up phase ships.
        Err(nros_platform_api::SchedError::Unsupported)
    }
}

// ============================================================================
// Random — xorshift (shared helpers from nros_platform_api::xorshift32)
// ============================================================================

static mut RNG_STATE: u32 = nros_platform_api::xorshift32::DEFAULT_SEED;

/// Seed the PRNG.
pub fn seed(value: u32) {
    unsafe { nros_platform_api::xorshift32::seed(&raw mut RNG_STATE, value) }
}

/// C-callable version for FreeRTOS startup.c to seed the platform RNG.
///
/// Without this, the C examples have no way to inject per-instance
/// entropy into the xorshift state, so two QEMU instances both start
/// with the default `0x12345678` seed → produce identical zenoh
/// session IDs → zenohd treats them as the same peer (`max_links=1`)
/// and rejects the second connection. The Rust path seeds via
/// `seed()` from `app_task_entry`; the C path now mirrors that.
#[unsafe(no_mangle)]
pub extern "C" fn nros_platform_freertos_seed_rng(value: u32) {
    seed(value);
}

fn next_u32() -> u32 {
    unsafe { nros_platform_api::xorshift32::next(&raw mut RNG_STATE) }
}

impl nros_platform_api::PlatformRandom for FreeRtosPlatform {
    fn random_u8() -> u8 {
        (next_u32() & 0xFF) as u8
    }
    fn random_u16() -> u16 {
        (next_u32() & 0xFFFF) as u16
    }
    fn random_u32() -> u32 {
        next_u32()
    }
    fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe {
            nros_platform_api::xorshift32::random_fill(&raw mut RNG_STATE, buf as *mut u8, len)
        }
    }
}

// ============================================================================
// Time — monotonic (no RTC)
// ============================================================================

impl nros_platform_api::PlatformTime for FreeRtosPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        <Self as PlatformClock>::clock_ms()
    }
    #[inline]
    fn time_since_epoch_secs() -> u32 {
        (<Self as PlatformClock>::clock_ms() / 1000) as u32
    }
    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        ((<Self as PlatformClock>::clock_ms() % 1000) * 1_000_000) as u32
    }
}

// ============================================================================
// Threading — FreeRTOS tasks, recursive mutexes, condvar
//
// Uses #[repr(C)] types from types.rs matching zenoh-pico's FreeRTOS structs.
// ============================================================================

/// Task wrapper trampoline — matches zenoh-pico's z_task_wrapper behavior.
/// Reads fun/arg from the _z_task_t struct, runs the function, signals
/// completion via event group, then suspends (deleted by joiner).
unsafe extern "C" fn task_wrapper(arg: *mut c_void) {
    let task = arg as *mut types::ZTask;
    unsafe {
        // Run the task function
        if let Some(fun) = (*task).fun {
            fun((*task).arg);
        }
        // Signal the joiner that the task has finished
        ffi::event_group_set_bits((*task).join_event, 1);
        // Suspend self — joiner will delete us (avoids race with vTaskDelete)
        ffi::task_suspend_current();
    }
}

/// Default task attributes (matches zenoh-pico's z_default_task_attr)
const DEFAULT_TASK_NAME: &core::ffi::CStr = c"nros";
const DEFAULT_PRIORITY: u32 = 3; // configMAX_PRIORITIES / 2
const DEFAULT_STACK_DEPTH: u32 = 5120;

impl FreeRtosPlatform {
    pub fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        let entry = match entry {
            Some(f) => f,
            None => return -1,
        };

        let t = task as *mut types::ZTask;
        unsafe {
            // Store fun/arg in the struct (task_wrapper reads them)
            (*t).fun = Some(entry);
            (*t).arg = arg;

            // Create join event group
            (*t).join_event = ffi::event_group_create();
            if (*t).join_event.is_null() {
                return -1;
            }

            // Read attributes (or use defaults)
            let (name, priority, stack_depth) = if !attr.is_null() {
                let a = attr as *const types::ZTaskAttr;
                ((*a).name, (*a).priority, (*a).stack_depth as u32)
            } else {
                (
                    DEFAULT_TASK_NAME.as_ptr().cast::<u8>(),
                    DEFAULT_PRIORITY,
                    DEFAULT_STACK_DEPTH,
                )
            };

            // Create the task — pass the ZTask struct as arg to task_wrapper
            let ret = ffi::xTaskCreate(
                task_wrapper,
                name as *const core::ffi::c_char,
                stack_depth,
                task, // task_wrapper receives the ZTask struct
                priority,
                &mut (*t).handle,
            );
            if ret != 1 {
                // pdPASS = 1
                ffi::event_group_delete((*t).join_event);
                return -1;
            }
        }
        0
    }

    pub fn task_join(task: *mut c_void) -> i8 {
        let t = task as *mut types::ZTask;
        unsafe {
            // Wait for task to signal completion
            ffi::event_group_wait_bits((*t).join_event, 1, u32::MAX);

            // Delete the suspended task
            ffi::enter_critical();
            if !(*t).handle.is_null() {
                ffi::vTaskDelete((*t).handle);
                (*t).handle = core::ptr::null_mut();
            }
            ffi::exit_critical();
        }
        0
    }

    pub fn task_detach(_task: *mut c_void) -> i8 {
        -1 // Not supported on FreeRTOS (same as C system.c)
    }

    pub fn task_cancel(task: *mut c_void) -> i8 {
        let t = task as *mut types::ZTask;
        unsafe {
            ffi::enter_critical();
            if !(*t).handle.is_null() {
                ffi::vTaskDelete((*t).handle);
                (*t).handle = core::ptr::null_mut();
            }
            ffi::exit_critical();
            // Signal joiners
            ffi::event_group_set_bits((*t).join_event, 1);
        }
        0
    }

    pub fn task_exit() {
        unsafe { ffi::vTaskDelete(core::ptr::null_mut()) };
    }

    pub fn task_free(task: *mut *mut c_void) {
        unsafe {
            let t = *task as *mut types::ZTask;
            if !t.is_null() {
                ffi::event_group_delete((*t).join_event);
                Self::dealloc(t as *mut c_void);
                *task = core::ptr::null_mut();
            }
        }
    }

    // -- Mutex (recursive) --
    // _z_mutex_t = { SemaphoreHandle_t handle } — single field at offset 0

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let mx = m as *mut types::ZMutex;
        let handle = ffi::create_recursive_mutex();
        if handle.is_null() {
            return -1;
        }
        unsafe { (*mx).handle = handle };
        0
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let handle = unsafe { (*mx).handle };
        if !handle.is_null() {
            ffi::semaphore_delete(handle);
        }
        0
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::take_recursive(unsafe { (*mx).handle }, u32::MAX);
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::take_recursive(unsafe { (*mx).handle }, 0);
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::give_recursive(unsafe { (*mx).handle });
        if ret == 1 { 0 } else { -1 }
    }

    // -- Recursive mutex (same implementation on FreeRTOS) --

    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        Self::mutex_init(m)
    }
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        Self::mutex_drop(m)
    }
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        Self::mutex_lock(m)
    }
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        Self::mutex_unlock(m)
    }

    // -- Condition variables (matching C system.c semantics) --
    // _z_condvar_t = { SemaphoreHandle_t mutex, SemaphoreHandle_t sem, int waiters }

    pub fn condvar_init(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            let mutex = ffi::create_mutex();
            let sem = ffi::create_counting_semaphore(u32::MAX, 0);
            if mutex.is_null() || sem.is_null() {
                if !mutex.is_null() {
                    ffi::semaphore_delete(mutex);
                }
                if !sem.is_null() {
                    ffi::semaphore_delete(sem);
                }
                return -1;
            }
            (*c).mutex = mutex;
            (*c).sem = sem;
            (*c).waiters = 0;
        }
        0
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let c = cv as *const types::ZCondvar;
        unsafe {
            ffi::semaphore_delete((*c).sem);
            ffi::semaphore_delete((*c).mutex);
        }
        0
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            if (*c).waiters > 0 {
                ffi::semaphore_give((*c).sem);
                (*c).waiters -= 1;
            }
            ffi::semaphore_give((*c).mutex);
        }
        0
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            while (*c).waiters > 0 {
                ffi::semaphore_give((*c).sem);
                (*c).waiters -= 1;
            }
            ffi::semaphore_give((*c).mutex);
        }
        0
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            // Increment waiter count
            ffi::semaphore_take((*c).mutex, u32::MAX);
            (*c).waiters += 1;
            ffi::semaphore_give((*c).mutex);

            // Release the caller's mutex and wait on the semaphore
            Self::mutex_unlock(m);
            ffi::semaphore_take((*c).sem, u32::MAX);
            Self::mutex_lock(m);
        }
        0
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        let c = cv as *mut types::ZCondvar;
        let now = Self::clock_ms();
        let timeout = abstime.saturating_sub(now) as u32;

        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            (*c).waiters += 1;
            ffi::semaphore_give((*c).mutex);

            Self::mutex_unlock(m);
            let ret = ffi::semaphore_take((*c).sem, timeout);
            Self::mutex_lock(m);

            if ret != 1 {
                // Timed out — decrement waiter count
                ffi::semaphore_take((*c).mutex, u32::MAX);
                (*c).waiters -= 1;
                ffi::semaphore_give((*c).mutex);
                return -1; // Z_ETIMEDOUT
            }
        }
        0
    }
}

impl nros_platform_api::PlatformThreading for FreeRtosPlatform {
    fn task_init(
        task: *mut core::ffi::c_void,
        attr: *mut core::ffi::c_void,
        entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        arg: *mut core::ffi::c_void,
    ) -> i8 {
        Self::task_init(task, attr, entry, arg)
    }
    fn task_join(task: *mut core::ffi::c_void) -> i8 {
        Self::task_join(task)
    }
    fn task_detach(task: *mut core::ffi::c_void) -> i8 {
        Self::task_detach(task)
    }
    fn task_cancel(task: *mut core::ffi::c_void) -> i8 {
        Self::task_cancel(task)
    }
    fn task_exit() {
        Self::task_exit()
    }
    fn task_free(task: *mut *mut core::ffi::c_void) {
        Self::task_free(task)
    }
    fn mutex_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_init(m)
    }
    fn mutex_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_drop(m)
    }
    fn mutex_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_lock(m)
    }
    fn mutex_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    fn mutex_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_unlock(m)
    }
    fn mutex_rec_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_init(m)
    }
    fn mutex_rec_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_drop(m)
    }
    fn mutex_rec_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_lock(m)
    }
    fn mutex_rec_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_try_lock(m)
    }
    fn mutex_rec_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_unlock(m)
    }
    fn condvar_init(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_init(cv)
    }
    fn condvar_drop(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_drop(cv)
    }
    fn condvar_signal(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal(cv)
    }
    fn condvar_signal_all(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal_all(cv)
    }
    fn condvar_wait(cv: *mut core::ffi::c_void, m: *mut core::ffi::c_void) -> i8 {
        Self::condvar_wait(cv, m)
    }
    fn condvar_wait_until(
        cv: *mut core::ffi::c_void,
        m: *mut core::ffi::c_void,
        abstime: u64,
    ) -> i8 {
        Self::condvar_wait_until(cv, m, abstime)
    }
}

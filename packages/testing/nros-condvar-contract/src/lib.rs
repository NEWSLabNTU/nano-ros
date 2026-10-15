//! Issue 1906 — the platform condvar contract, checked on a real RTOS.
//!
//! `nros-platform-cffi/tests/c_port_posix.rs` checks the POSIX port on the
//! host. ThreadX and FreeRTOS have no host harness, so this runs the same
//! checks inside an image of each (`nros-tests/bins/condvar-contract-*`),
//! through the `nros_platform_*` C ABI the board links, and reports each
//! result as one line. The host test reads those lines.
//!
//! The checks:
//!
//! 1. A timed wait nobody signals returns `1`, and not before its deadline.
//!    Issue 1906 fixed both halves: FreeRTOS returned `-1`, and both ports
//!    rounded the remaining time DOWN to a tick.
//! 2. A broadcast wakes every waiter. ThreadX woke one, so the rest slept.
//! 3. Optional, issue 1907: a wait under a mutex held twice is refused (`-1`)
//!    rather than kept across the wait. Only ports that can see the lock count
//!    refuse it, so the image says whether to run this check.
//! 4. Feature `zenoh-condvar-wait`, phase-502 W2: zenoh-pico's own
//!    `_z_condvar_wait`, with nobody signalling, returns `0` (a spurious wake)
//!    after one slice instead of waiting forever. See `run_zenoh_wait`.
//! 5. Same feature, phase-502 W6: zenoh-pico's `_z_condvar_wait_until` with a
//!    deadline set before the tick count wrapped times out at once. Comparing
//!    raw tick values read it as about one full tick range in the future. See
//!    `run_zenoh_wait_until_wrapped`.

#![no_std]

use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

unsafe extern "C" {
    fn nros_platform_clock_ns() -> u64;
    fn nros_platform_sleep_ms(ms: usize);
    fn nros_platform_mutex_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_unlock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_unlock(m: *mut c_void) -> i8;
    fn nros_platform_condvar_init(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_drop(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_signal_all(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime_ms: u64) -> i8;
    fn nros_platform_task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> i8;
    fn nros_platform_task_join(task: *mut c_void) -> i8;
    fn nros_platform_task_attr_init(attr: *mut nros_platform_task_attr_t);
    #[cfg(feature = "zenoh-condvar-wait")]
    fn nros_platform_wake_park_granularity_us() -> u64;
}

/// `nros_platform_task_attr_t` (`nros/platform.h`), hand-mirrored and named as
/// the header names it, which `check-platform-abi-mirror` compares against the
/// generated bindings.
#[allow(non_camel_case_types)]
#[repr(C)]
struct nros_platform_task_attr_t {
    name: *const u8,
    stack_bytes: usize,
    stack_mem: *mut c_void,
    priority: i32,
    core: i8,
    flags: u8,
}

/// A waiter needs a few hundred bytes of stack. The port raises this to its own
/// floor; without it FreeRTOS charges its 128 KiB default to each task.
const WAITER_STACK_BYTES: usize = 4096;

/// Opaque storage at least `NROS_PLATFORM_{MUTEX,CONDVAR}_STORAGE_SIZE` (256)
/// or `NROS_PLATFORM_TASK_STORAGE_SIZE` (512), aligned to
/// `NROS_PLATFORM_STORAGE_ALIGN` (8).
#[repr(C, align(8))]
struct Storage<const N: usize>(UnsafeCell<[u8; N]>);

// SAFETY: the bytes are only ever handed to the platform, whose primitives do
// their own synchronisation; this crate never reads or writes them.
unsafe impl<const N: usize> Sync for Storage<N> {}

impl<const N: usize> Storage<N> {
    const fn new() -> Self {
        Self(UnsafeCell::new([0; N]))
    }
    fn ptr(&self) -> *mut c_void {
        self.0.get().cast()
    }
}

const WAITERS: usize = 3;

static CV: Storage<256> = Storage::new();
static M: Storage<256> = Storage::new();
static REC: Storage<256> = Storage::new();
static TASKS: [Storage<512>; WAITERS] = [Storage::new(), Storage::new(), Storage::new()];

static WAITING: AtomicU32 = AtomicU32::new(0);
static RELEASED: AtomicBool = AtomicBool::new(false);
static WOKEN: AtomicU32 = AtomicU32::new(0);

/// Marker every passing check prints, for `nros_tests::output`.
pub const PASS_PREFIX: &str = "condvar-contract PASS ";
/// Marker every failing check prints.
pub const FAIL_PREFIX: &str = "condvar-contract FAIL ";
/// Printed once, after every check passed.
pub const ALL_PASSED: &str = "condvar-contract: all checks passed";

fn now_ms() -> u64 {
    // SAFETY: plain call into the board's clock.
    unsafe { nros_platform_clock_ns() / 1_000_000 }
}

extern "C" fn waiter(_arg: *mut c_void) -> *mut c_void {
    // SAFETY: CV and M were initialised by `run` before any waiter spawned.
    unsafe {
        nros_platform_mutex_lock(M.ptr());
        WAITING.fetch_add(1, Ordering::SeqCst);
        // Bounded, so a lost broadcast FAILS the check instead of hanging it.
        let deadline = now_ms() + 5_000;
        // Count a waiter only if a WAKE released it. One that ran out its own
        // deadline and then saw `RELEASED` missed the broadcast: counting it
        // let a one-waiter broadcast pass, measured with issue 1906's ThreadX
        // defect put back.
        let mut timed_out = false;
        while !RELEASED.load(Ordering::SeqCst) {
            if nros_platform_condvar_wait_until(CV.ptr(), M.ptr(), deadline) == 1 {
                timed_out = true;
                break;
            }
        }
        if RELEASED.load(Ordering::SeqCst) && !timed_out {
            WOKEN.fetch_add(1, Ordering::SeqCst);
        }
        nros_platform_mutex_unlock(M.ptr());
    }
    ptr::null_mut()
}

/// Runs every check, calling `report` with one line per check, and returns
/// whether all of them passed. `check_held_twice` runs the issue-1907 refusal
/// check, for a port that can see a mutex's lock count.
pub fn run(report: &mut dyn FnMut(&str, bool, &str), check_held_twice: bool) -> bool {
    let mut ok = true;
    // SAFETY: each primitive is initialised before use and dropped after; the
    // storage is static and never shared with anything else.
    unsafe {
        if nros_platform_condvar_init(CV.ptr()) != 0 || nros_platform_mutex_init(M.ptr()) != 0 {
            report("init", false, "condvar_init or mutex_init failed");
            return false;
        }

        // 1. A timed wait with no signaller returns 1, never early.
        const WAIT_MS: u64 = 50;
        nros_platform_mutex_lock(M.ptr());
        let start = now_ms();
        let deadline = start + WAIT_MS;
        let rc = loop {
            let rc = nros_platform_condvar_wait_until(CV.ptr(), M.ptr(), deadline);
            if rc != 0 {
                break rc; // 0 is a spurious wake: wait again for the same deadline
            }
        };
        let elapsed = now_ms() - start;
        nros_platform_mutex_unlock(M.ptr());
        let timed_ok = rc == 1 && elapsed >= WAIT_MS;
        report(
            "timed-wait",
            timed_ok,
            if rc != 1 {
                "returned something other than the timeout code 1"
            } else if elapsed < WAIT_MS {
                "timed out before its deadline"
            } else {
                "returned 1 at or after its deadline"
            },
        );
        ok &= timed_ok;

        // 2. A broadcast wakes every waiter.
        let mut attr = core::mem::MaybeUninit::<nros_platform_task_attr_t>::uninit();
        nros_platform_task_attr_init(attr.as_mut_ptr());
        let mut attr = attr.assume_init();
        attr.name = c"cv-waiter".as_ptr().cast();
        attr.stack_bytes = WAITER_STACK_BYTES;
        let mut spawned = 0;
        for t in &TASKS {
            let a = (&mut attr as *mut nros_platform_task_attr_t).cast();
            if nros_platform_task_init(t.ptr(), a, waiter, ptr::null_mut()) == 0 {
                spawned += 1;
            }
        }
        if spawned != WAITERS {
            report("broadcast", false, "could not spawn the waiter tasks");
            return false;
        }
        // Each waiter counts itself while holding M, so once all three have
        // counted and M is ours, all three are inside the wait.
        let give_up = now_ms() + 5_000;
        while (WAITING.load(Ordering::SeqCst) as usize) < WAITERS && now_ms() < give_up {
            nros_platform_sleep_ms(1);
        }
        nros_platform_mutex_lock(M.ptr());
        RELEASED.store(true, Ordering::SeqCst);
        let sent = now_ms();
        let bc = nros_platform_condvar_signal_all(CV.ptr());
        nros_platform_mutex_unlock(M.ptr());
        for t in &TASKS {
            nros_platform_task_join(t.ptr());
        }
        // Every waiter must be released by the broadcast itself, promptly: far
        // inside the 5 s the waiters would otherwise wait.
        let took = now_ms() - sent;
        let woken = WOKEN.load(Ordering::SeqCst) as usize;
        let bc_ok = bc == 0 && woken == WAITERS && took < 1_000;
        report(
            "broadcast",
            bc_ok,
            if bc_ok {
                "every waiter woke"
            } else if woken != WAITERS {
                "a waiter missed the broadcast"
            } else {
                "the waiters woke, but not promptly"
            },
        );
        ok &= bc_ok;

        // 3. Issue 1907: a wait under a mutex held twice is refused.
        if check_held_twice {
            nros_platform_mutex_rec_init(REC.ptr());
            nros_platform_mutex_rec_lock(REC.ptr());
            nros_platform_mutex_rec_lock(REC.ptr());
            let rc = nros_platform_condvar_wait_until(CV.ptr(), REC.ptr(), now_ms() + 10);
            nros_platform_mutex_rec_unlock(REC.ptr());
            nros_platform_mutex_rec_unlock(REC.ptr());
            nros_platform_mutex_rec_drop(REC.ptr());
            let refused = rc == -1;
            report(
                "held-twice",
                refused,
                if refused {
                    "the wait was refused"
                } else {
                    "the wait ran under a doubly held mutex"
                },
            );
            ok &= refused;
        }

        nros_platform_condvar_drop(CV.ptr());
        nros_platform_mutex_drop(M.ptr());
    }
    ok
}

#[cfg(feature = "zenoh-condvar-wait")]
mod zenoh_wait {
    use super::*;

    // zenoh-pico's own sync layer (`zenoh-pico/system/common/platform.h`),
    // `z_result_t` being `int8_t`. On FreeRTOS these are the vendored
    // `system/freertos/system.c` primitives, whose structs this crate never
    // reads: it hands them storage larger than either.
    unsafe extern "C" {
        pub(super) fn _z_mutex_init(m: *mut c_void) -> i8;
        pub(super) fn _z_mutex_drop(m: *mut c_void) -> i8;
        pub(super) fn _z_mutex_lock(m: *mut c_void) -> i8;
        pub(super) fn _z_mutex_unlock(m: *mut c_void) -> i8;
        pub(super) fn _z_condvar_init(cv: *mut c_void) -> i8;
        pub(super) fn _z_condvar_drop(cv: *mut c_void) -> i8;
        pub(super) fn _z_condvar_signal_all(cv: *mut c_void) -> i8;
        pub(super) fn _z_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;
        pub(super) fn _z_condvar_wait_until(
            cv: *mut c_void,
            m: *mut c_void,
            abstime: *const u32,
        ) -> i8;
        pub(super) fn z_clock_now() -> u32;
    }

    /// `Z_ETIMEDOUT` (`zenoh-pico/utils/result.h`).
    pub const Z_ETIMEDOUT: i8 = -71;

    /// The slice `Z_FEATURE_NROS_BOUNDED_CONDVAR_WAIT` bounds one wait to
    /// (`Z_NROS_CONDVAR_WAIT_SLICE_MS`, the alias shim's
    /// `NROS_ZP_CONDVAR_SLICE_MS`).
    pub const SLICE_MS: u64 = 1_000;
    /// A wait still blocked this long is rescued by a signal, so the check
    /// FAILS on an unbounded wait instead of hanging the image.
    pub const RESCUE_MS: u64 = 10 * SLICE_MS;
    /// "Well under ten slices": a bounded wait returns after one.
    pub const UPPER_MS: u64 = 3 * SLICE_MS;

    pub(super) static ZCV: Storage<256> = Storage::new();
    pub(super) static ZM: Storage<256> = Storage::new();
    pub(super) static RESCUER: Storage<512> = Storage::new();
    pub(super) static WAIT_DONE: AtomicBool = AtomicBool::new(false);
    pub(super) static RESCUED: AtomicBool = AtomicBool::new(false);

    pub(super) extern "C" fn rescuer(_arg: *mut c_void) -> *mut c_void {
        let give_up = now_ms() + RESCUE_MS;
        while !WAIT_DONE.load(Ordering::SeqCst) && now_ms() < give_up {
            // SAFETY: plain call into the board's sleep.
            unsafe { nros_platform_sleep_ms(10) };
        }
        if !WAIT_DONE.load(Ordering::SeqCst) {
            RESCUED.store(true, Ordering::SeqCst);
            // SAFETY: ZCV / ZM were initialised before this task spawned and
            // are dropped only after it is joined.
            unsafe {
                _z_mutex_lock(ZM.ptr());
                _z_condvar_signal_all(ZCV.ptr());
                _z_mutex_unlock(ZM.ptr());
            }
        }
        ptr::null_mut()
    }
}

/// One zenoh-pico wait, guarded by a rescuer task that signals after
/// `RESCUE_MS` so an unbounded wait reports a FAIL instead of hanging the
/// image. Returns `(rc, elapsed_ms, rescued)`, or the setup failure.
#[cfg(feature = "zenoh-condvar-wait")]
fn rescued_zenoh_wait(
    wait: &mut dyn FnMut(*mut c_void, *mut c_void) -> i8,
) -> Result<(i8, u64, bool), &'static str> {
    use zenoh_wait::*;
    WAIT_DONE.store(false, Ordering::SeqCst);
    RESCUED.store(false, Ordering::SeqCst);
    // SAFETY: each primitive is initialised before use and dropped after the
    // only other user (the rescuer) is joined; the storage is static.
    unsafe {
        if _z_condvar_init(ZCV.ptr()) != 0 || _z_mutex_init(ZM.ptr()) != 0 {
            return Err("_z_condvar_init or _z_mutex_init failed");
        }
        let mut attr = core::mem::MaybeUninit::<nros_platform_task_attr_t>::uninit();
        nros_platform_task_attr_init(attr.as_mut_ptr());
        let mut attr = attr.assume_init();
        attr.name = c"zcv-rescuer".as_ptr().cast();
        attr.stack_bytes = WAITER_STACK_BYTES;
        let a = (&mut attr as *mut nros_platform_task_attr_t).cast();
        if nros_platform_task_init(RESCUER.ptr(), a, rescuer, ptr::null_mut()) != 0 {
            return Err("could not spawn the rescuer task");
        }

        _z_mutex_lock(ZM.ptr());
        let start = now_ms();
        let rc = wait(ZCV.ptr(), ZM.ptr());
        let elapsed = now_ms() - start;
        _z_mutex_unlock(ZM.ptr());
        WAIT_DONE.store(true, Ordering::SeqCst);
        nros_platform_task_join(RESCUER.ptr());
        _z_condvar_drop(ZCV.ptr());
        _z_mutex_drop(ZM.ptr());
        Ok((rc, elapsed, RESCUED.load(Ordering::SeqCst)))
    }
}

/// Phase-502 W2: zenoh-pico's `_z_condvar_wait` with no signaller returns `0`
/// after one slice — not before (less the one tick the port declares as its
/// wake granularity, since FreeRTOS counts the current partial tick), and well
/// under ten. With the bound off the wait is forever; a rescuer task signals
/// after ten slices, so that shape reports a FAIL rather than hanging.
#[cfg(feature = "zenoh-condvar-wait")]
pub fn run_zenoh_wait(report: &mut dyn FnMut(&str, bool, &str)) -> bool {
    use zenoh_wait::*;
    // SAFETY: both pointers are the initialised primitives the harness passes.
    let (rc, elapsed, rescued) =
        match rescued_zenoh_wait(&mut |cv, m| unsafe { _z_condvar_wait(cv, m) }) {
            Ok(r) => r,
            Err(e) => {
                report("zenoh-wait", false, e);
                return false;
            }
        };
    // SAFETY: plain query of the port's declared wake granularity.
    let tick_ms = unsafe { nros_platform_wake_park_granularity_us() }.div_ceil(1_000);
    let lower = SLICE_MS.saturating_sub(tick_ms);
    let ok = rc == 0 && !rescued && elapsed >= lower && elapsed < UPPER_MS;
    let mut buf = Detail::new();
    let _ = core::fmt::write(
        &mut buf,
        format_args!(
            "rc={rc} elapsed_ms={elapsed} bounds=[{lower},{UPPER_MS}) rescued={rescued}: {}",
            if ok {
                "returned after one slice"
            } else if rescued {
                "the wait returned only when rescued: it is unbounded"
            } else if rc != 0 {
                "returned something other than 0"
            } else if elapsed < lower {
                "returned before one slice"
            } else {
                "returned, but not within a few slices"
            }
        ),
    );
    report("zenoh-wait", ok, buf.as_str());
    ok
}

/// Phase-502 W6: `_z_condvar_wait_until` with a deadline set before the tick
/// count wrapped returns `Z_ETIMEDOUT` at once.
///
/// The deadline is "1000 ticks before tick 0", so it has passed by the
/// image's whole uptime plus 1000 ticks; numerically it is near the top of the
/// tick range, above any tick count this image reaches. The vendored FreeRTOS
/// wait compared the raw values, read it as about 2^32 ticks ahead and
/// blocked until rescued. Ticks are 32-bit on every FreeRTOS board here
/// (`configUSE_16_BIT_TICKS 0`), which is what `z_clock_t` is.
#[cfg(feature = "zenoh-condvar-wait")]
pub fn run_zenoh_wait_until_wrapped(report: &mut dyn FnMut(&str, bool, &str)) -> bool {
    use zenoh_wait::*;
    const NAME: &str = "zenoh-wait-until-wrapped";
    let deadline: u32 = 0u32.wrapping_sub(1_000);
    // SAFETY: plain query of the zenoh-pico clock.
    let now = unsafe { z_clock_now() };
    // SAFETY: both pointers are the initialised primitives the harness passes,
    // and `deadline` outlives the call.
    let (rc, elapsed, rescued) =
        match rescued_zenoh_wait(&mut |cv, m| unsafe { _z_condvar_wait_until(cv, m, &deadline) }) {
            Ok(r) => r,
            Err(e) => {
                report(NAME, false, e);
                return false;
            }
        };
    let ok = rc == Z_ETIMEDOUT && !rescued && elapsed < SLICE_MS;
    let mut buf = Detail::new();
    let _ = core::fmt::write(
        &mut buf,
        format_args!(
            "rc={rc} now_ticks={now} deadline_ticks={deadline} elapsed_ms={elapsed} \
             rescued={rescued}: {}",
            if ok {
                "a deadline from before the wrap timed out at once"
            } else if rescued {
                "the wait read a past deadline as far in the future"
            } else if rc != Z_ETIMEDOUT {
                "returned something other than Z_ETIMEDOUT"
            } else {
                "timed out, but not at once"
            }
        ),
    );
    report(NAME, ok, buf.as_str());
    ok
}

/// A fixed buffer for one report detail, so the line can carry the measured
/// numbers without an allocator.
#[cfg(feature = "zenoh-condvar-wait")]
struct Detail {
    buf: [u8; 160],
    len: usize,
}

#[cfg(feature = "zenoh-condvar-wait")]
impl Detail {
    fn new() -> Self {
        Self {
            buf: [0; 160],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("<detail>")
    }
}

#[cfg(feature = "zenoh-condvar-wait")]
impl core::fmt::Write for Detail {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

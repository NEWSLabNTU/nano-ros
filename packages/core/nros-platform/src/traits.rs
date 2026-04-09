//! Platform capability sub-traits.
//!
//! Each trait covers an independent system capability. Platform
//! implementations pick which traits to implement based on what the
//! hardware/RTOS provides. RMW shim crates declare trait bounds for
//! the capabilities they need.

use core::ffi::{c_int, c_void};

// ============================================================================
// Clock (required by all RMW backends)
// ============================================================================

/// Monotonic clock.
///
/// The most critical platform primitive. Must be backed by a hardware timer
/// or OS tick — never by a software counter that only advances when polled.
pub trait PlatformClock {
    /// Returns monotonic time in milliseconds.
    fn clock_ms() -> u64;

    /// Returns monotonic time in microseconds.
    fn clock_us() -> u64;
}

// ============================================================================
// Heap allocation (zenoh-pico requires ~64 KB heap)
// ============================================================================

/// Heap memory allocation.
pub trait PlatformAlloc {
    /// Allocate `size` bytes. Returns null on failure.
    fn alloc(size: usize) -> *mut c_void;

    /// Reallocate a previously allocated block. Returns null on failure.
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;

    /// Free a previously allocated block.
    fn dealloc(ptr: *mut c_void);
}

// ============================================================================
// Sleep / delay
// ============================================================================

/// Sleep primitives.
///
/// On bare-metal with smoltcp, implementations should poll the network
/// stack during busy-wait sleep to avoid missing packets.
pub trait PlatformSleep {
    /// Sleep for the given number of microseconds.
    fn sleep_us(us: usize);

    /// Sleep for the given number of milliseconds.
    fn sleep_ms(ms: usize);

    /// Sleep for the given number of seconds.
    fn sleep_s(s: usize);
}

// ============================================================================
// Random number generation
// ============================================================================

/// Pseudo-random number generation.
///
/// A simple xorshift32 PRNG is sufficient. Seed with hardware entropy
/// (RNG peripheral, ADC noise, wall-clock time) during platform init.
pub trait PlatformRandom {
    fn random_u8() -> u8;
    fn random_u16() -> u16;
    fn random_u32() -> u32;
    fn random_u64() -> u64;

    /// Fill buffer with random bytes.
    fn random_fill(buf: *mut c_void, len: usize);
}

// ============================================================================
// Wall-clock time (for logging, not timing-critical)
// ============================================================================

/// Time since epoch.
#[repr(C)]
pub struct TimeSinceEpoch {
    pub secs: u32,
    pub nanos: u32,
}

/// Wall-clock / system time.
///
/// Used for logging timestamps and `z_time_now_as_str()`.
/// On bare-metal without an RTC, return monotonic time or zeros.
pub trait PlatformTime {
    /// Returns system time in milliseconds.
    fn time_now_ms() -> u64;

    /// Returns time since epoch.
    fn time_since_epoch() -> TimeSinceEpoch;
}

// ============================================================================
// Threading (multi-threaded platforms only)
// ============================================================================

/// Opaque task handle.
///
/// Platform implementations define the actual layout. Must be
/// at least `size_of::<*mut c_void>()` bytes for a handle/pointer.
#[repr(C)]
pub struct TaskHandle {
    _opaque: [u8; 64],
}

/// Task creation attributes.
#[repr(C)]
pub struct TaskAttr {
    _opaque: [u8; 64],
}

/// Opaque mutex handle.
#[repr(C)]
pub struct MutexHandle {
    _opaque: [u8; 32],
}

/// Opaque recursive mutex handle.
#[repr(C)]
pub struct RecursiveMutexHandle {
    _opaque: [u8; 32],
}

/// Opaque condition variable handle.
#[repr(C)]
pub struct CondvarHandle {
    _opaque: [u8; 48],
}

/// Threading primitives: tasks, mutexes, and condition variables.
///
/// For single-threaded platforms (bare-metal), all operations should be
/// no-ops returning success (0), except `task_init` which should return -1.
pub trait PlatformThreading {
    // -- Tasks --

    /// Spawn a new task. Returns 0 on success, -1 on failure.
    fn task_init(
        task: *mut TaskHandle,
        attr: *mut TaskAttr,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8;

    fn task_join(task: *mut TaskHandle) -> i8;
    fn task_detach(task: *mut TaskHandle) -> i8;
    fn task_cancel(task: *mut TaskHandle) -> i8;
    fn task_exit();
    fn task_free(task: *mut *mut TaskHandle);

    // -- Mutex --

    fn mutex_init(m: *mut MutexHandle) -> i8;
    fn mutex_drop(m: *mut MutexHandle) -> i8;
    fn mutex_lock(m: *mut MutexHandle) -> i8;
    fn mutex_try_lock(m: *mut MutexHandle) -> i8;
    fn mutex_unlock(m: *mut MutexHandle) -> i8;

    // -- Recursive mutex --

    fn mutex_rec_init(m: *mut RecursiveMutexHandle) -> i8;
    fn mutex_rec_drop(m: *mut RecursiveMutexHandle) -> i8;
    fn mutex_rec_lock(m: *mut RecursiveMutexHandle) -> i8;
    fn mutex_rec_try_lock(m: *mut RecursiveMutexHandle) -> i8;
    fn mutex_rec_unlock(m: *mut RecursiveMutexHandle) -> i8;

    // -- Condition variables --

    fn condvar_init(cv: *mut CondvarHandle) -> i8;
    fn condvar_drop(cv: *mut CondvarHandle) -> i8;
    fn condvar_signal(cv: *mut CondvarHandle) -> i8;
    fn condvar_signal_all(cv: *mut CondvarHandle) -> i8;
    fn condvar_wait(cv: *mut CondvarHandle, m: *mut MutexHandle) -> i8;

    /// Wait with absolute timeout (milliseconds since boot).
    fn condvar_wait_until(cv: *mut CondvarHandle, m: *mut MutexHandle, abstime: u64) -> i8;
}

/// Network poll callback for bare-metal platforms using smoltcp.
///
/// Not required for platforms with OS-level networking (POSIX, Zephyr, NuttX).
pub trait PlatformNetworkPoll {
    /// Poll the network stack to process pending I/O.
    fn network_poll();

    /// Monotonic clock for smoltcp TCP/IP timestamping.
    /// Typically delegates to `PlatformClock::clock_ms()`.
    fn smoltcp_clock_now_ms() -> u64;
}

// ============================================================================
// libc stubs (bare-metal only)
// ============================================================================

/// Standard C library functions needed by zenoh-pico on bare-metal targets.
///
/// Platforms with a C runtime (RTOS, POSIX) do NOT need to implement this.
pub trait PlatformLibc {
    fn strlen(s: *const u8) -> usize;
    fn strcmp(s1: *const u8, s2: *const u8) -> c_int;
    fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> c_int;
    fn strchr(s: *const u8, c: c_int) -> *mut u8;
    fn strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(dest: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int;
    fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void;
    fn strtoul(nptr: *const u8, endptr: *mut *mut u8, base: c_int) -> core::ffi::c_ulong;
    fn errno_ptr() -> *mut c_int;
}

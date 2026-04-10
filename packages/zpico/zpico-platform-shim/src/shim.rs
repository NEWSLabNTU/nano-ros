use core::ffi::{c_char, c_ulong, c_void};

use nros_platform::ConcretePlatform;

type P = ConcretePlatform;

// ============================================================================
// Clock
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> usize {
    P::clock_ms() as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_elapsed_us(time: *const usize) -> c_ulong {
    let prev = unsafe { *time } as u64;
    let now = P::clock_ms();
    (now.wrapping_sub(prev) * 1000) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_elapsed_ms(time: *const usize) -> c_ulong {
    let prev = unsafe { *time } as u64;
    let now = P::clock_ms();
    now.wrapping_sub(prev) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_elapsed_s(time: *const usize) -> c_ulong {
    let prev = unsafe { *time } as u64;
    let now = P::clock_ms();
    (now.wrapping_sub(prev) / 1000) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_advance_us(clock: *mut usize, duration: c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add((duration as usize) / 1000);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_advance_ms(clock: *mut usize, duration: c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add(duration as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_advance_s(clock: *mut usize, duration: c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add((duration as usize) * 1000);
    }
}

// ============================================================================
// Memory
// smoltcp transport clock (used by zpico-smoltcp for TCP/IP timestamping)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    P::clock_ms()
}

// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn z_malloc(size: usize) -> *mut c_void {
    P::alloc(size)
}

#[unsafe(no_mangle)]
pub extern "C" fn z_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    P::realloc(ptr, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn z_free(ptr: *mut c_void) {
    P::dealloc(ptr)
}

// ============================================================================
// Sleep
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_us(time: usize) -> i8 {
    P::sleep_us(time);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_ms(time: usize) -> i8 {
    P::sleep_ms(time);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_s(time: usize) -> i8 {
    P::sleep_s(time);
    0
}

// ============================================================================
// Random
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u8() -> u8 {
    P::random_u8()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u16() -> u16 {
    P::random_u16()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u32() -> u32 {
    P::random_u32()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u64() -> u64 {
    P::random_u64()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_fill(buf: *mut c_void, len: usize) {
    P::random_fill(buf, len)
}

// ============================================================================
// Time (wall clock)
// ============================================================================

#[repr(C)]
pub struct ZTimeSinceEpoch {
    pub secs: u32,
    pub nanos: u32,
}

#[unsafe(no_mangle)]
pub extern "C" fn z_time_now() -> u64 {
    P::time_now_ms()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_time_now_as_str(buf: *mut c_char, _buflen: c_ulong) -> *const c_char {
    // Minimal stub — write "0" into the buffer
    if !buf.is_null() {
        unsafe {
            *buf = b'0' as c_char;
            *buf.add(1) = 0;
        }
    }
    buf as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn z_time_elapsed_us(time: *const u64) -> c_ulong {
    let prev = unsafe { *time };
    let now = P::time_now_ms();
    (now.wrapping_sub(prev) * 1000) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn z_time_elapsed_ms(time: *const u64) -> c_ulong {
    let prev = unsafe { *time };
    let now = P::time_now_ms();
    now.wrapping_sub(prev) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn z_time_elapsed_s(time: *const u64) -> c_ulong {
    let prev = unsafe { *time };
    let now = P::time_now_ms();
    (now.wrapping_sub(prev) / 1000) as c_ulong
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_get_time_since_epoch(t: *mut ZTimeSinceEpoch) -> i8 {
    let epoch = nros_platform::TimeSinceEpoch {
        secs: P::time_since_epoch_secs(),
        nanos: P::time_since_epoch_nanos(),
    };
    unsafe {
        (*t).secs = epoch.secs;
        (*t).nanos = epoch.nanos;
    }
    0
}

// ============================================================================
// Threading — tasks
// ============================================================================

// Opaque types matching zenoh-pico's expectations.
// The actual layout is defined by the platform backend.
#[repr(C)]
pub struct ZTask {
    _opaque: [u8; 64],
}

#[repr(C)]
pub struct ZTaskAttr {
    _opaque: [u8; 64],
}

#[repr(C)]
pub struct ZMutex {
    _opaque: [u8; 64],
}

#[repr(C)]
pub struct ZMutexRec {
    _opaque: [u8; 64],
}

#[repr(C)]
pub struct ZCondvar {
    _opaque: [u8; 64],
}

// Task symbols are skipped for ThreadX — provided by C task.c instead,
// because _z_task_t struct layout (TX_THREAD + embedded stack) is needed
// for tx_thread_create() and the trampoline.
#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_init(
    task: *mut ZTask,
    attr: *mut ZTaskAttr,
    fun: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    arg: *mut c_void,
) -> i8 {
    P::task_init(task as *mut c_void, attr as *mut c_void, fun, arg)
}

#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_join(task: *mut ZTask) -> i8 {
    P::task_join(task as *mut c_void)
}

#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_detach(task: *mut ZTask) -> i8 {
    P::task_detach(task as *mut c_void)
}

#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_cancel(task: *mut ZTask) -> i8 {
    P::task_cancel(task as *mut c_void)
}

#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_exit() {
    P::task_exit()
}

#[cfg(not(feature = "skip-task-symbols"))]
#[unsafe(no_mangle)]
pub extern "C" fn _z_task_free(task: *mut *mut ZTask) {
    P::task_free(task as *mut *mut c_void)
}

// ============================================================================
// Threading — mutex
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_init(m: *mut ZMutex) -> i8 {
    P::mutex_init(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_drop(m: *mut ZMutex) -> i8 {
    P::mutex_drop(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_lock(m: *mut ZMutex) -> i8 {
    P::mutex_lock(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_try_lock(m: *mut ZMutex) -> i8 {
    P::mutex_try_lock(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_unlock(m: *mut ZMutex) -> i8 {
    P::mutex_unlock(m as *mut c_void)
}

// ============================================================================
// Threading — recursive mutex
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_init(m: *mut ZMutexRec) -> i8 {
    P::mutex_rec_init(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_drop(m: *mut ZMutexRec) -> i8 {
    P::mutex_rec_drop(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_lock(m: *mut ZMutexRec) -> i8 {
    P::mutex_rec_lock(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_try_lock(m: *mut ZMutexRec) -> i8 {
    P::mutex_rec_try_lock(m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_unlock(m: *mut ZMutexRec) -> i8 {
    P::mutex_rec_unlock(m as *mut c_void)
}

// ============================================================================
// Threading — condition variables
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_init(cv: *mut ZCondvar) -> i8 {
    P::condvar_init(cv as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_drop(cv: *mut ZCondvar) -> i8 {
    P::condvar_drop(cv as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_signal(cv: *mut ZCondvar) -> i8 {
    P::condvar_signal(cv as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_signal_all(cv: *mut ZCondvar) -> i8 {
    P::condvar_signal_all(cv as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_wait(cv: *mut ZCondvar, m: *mut ZMutex) -> i8 {
    P::condvar_wait(cv as *mut c_void, m as *mut c_void)
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_wait_until(
    cv: *mut ZCondvar,
    m: *mut ZMutex,
    abstime: *const u64,
) -> i8 {
    let t = unsafe { *abstime };
    P::condvar_wait_until(cv as *mut c_void, m as *mut c_void, t)
}

// ============================================================================
// Socket helpers (bare-metal only — RTOS platforms provide these in C network.c)
// ============================================================================

#[cfg(feature = "socket-stubs")]
mod socket_stubs {
    use core::ffi::c_void;

    #[repr(C)]
    pub struct ZSysNetSocket {
        pub _handle: i8,
        pub _connected: bool,
    }

    #[repr(C)]
    pub struct ZMutexRecRef {
        _unused: u8,
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn _z_socket_set_non_blocking(_sock: *const ZSysNetSocket) -> i8 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn _z_socket_accept(
        _sock_in: *const ZSysNetSocket,
        _sock_out: *mut ZSysNetSocket,
    ) -> i8 {
        -1 // Not supported — client-mode only
    }

    #[cfg(feature = "smoltcp")]
    #[unsafe(no_mangle)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub extern "C" fn _z_socket_close(sock: *mut ZSysNetSocket) {
        unsafe extern "C" {
            fn _z_close_tcp(sock: *mut ZSysNetSocket);
        }
        if sock.is_null() {
            return;
        }
        let handle = unsafe { (*sock)._handle };
        if handle >= 0 {
            unsafe { _z_close_tcp(sock) };
        }
    }

    #[cfg(not(feature = "smoltcp"))]
    #[unsafe(no_mangle)]
    pub extern "C" fn _z_socket_close(_sock: *mut ZSysNetSocket) {}

    #[cfg(feature = "smoltcp")]
    #[unsafe(no_mangle)]
    pub extern "C" fn _z_socket_wait_event(_peers: *mut c_void, _mutex: *mut ZMutexRecRef) -> i8 {
        unsafe extern "C" {
            fn smoltcp_poll() -> i32;
        }
        unsafe { smoltcp_poll() };
        0
    }

    #[cfg(not(feature = "smoltcp"))]
    #[unsafe(no_mangle)]
    pub extern "C" fn _z_socket_wait_event(_peers: *mut c_void, _mutex: *mut ZMutexRecRef) -> i8 {
        0
    }
}

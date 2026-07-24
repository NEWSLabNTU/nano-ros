//! Rust mirror of the canonical C ABI in `<nros/platform.h>`.
//!
//! Every nros binary links exactly one platform implementation; the
//! free `extern "C"` symbols declared below are resolved at link time.
//! There is no runtime registration step. To inject a platform from
//! C, drop a translation unit defining the symbols (or link against
//! a static library that does).
//!
//! Rust platform crates implement the [`nros_platform_api`] traits as
//! before; a sibling `-cffi` shim crate re-exports the Rust impl as
//! `#[unsafe(no_mangle)] extern "C"` symbols matching the names in the
//! header. That separation lets the same Rust impl serve both
//! trait-driven Rust callers and C-ABI consumers.
//!
//! # Usage
//!
//! - C implementor: implement the functions in `<nros/platform.h>` and
//!   link against the nros binary.
//! - Rust consumer: enable the `platform-cffi` feature on
//!   `nros-platform`; [`CffiPlatform`] dispatches every trait call to
//!   the linked C symbols.
//!
//! # Companion
//!
//! Platform sits one tier below RMW. The Phase 117 RMW vtable
//! (`<nros/rmw_vtable.h>`) is a runtime-pluggable struct; the
//! platform layer is link-time-bound free symbols. Different choice
//! because RMW backends genuinely swap per session (zenoh vs cyclonedds
//! vs xrce in the same binary at test time) while a platform is fixed
//! for the life of a binary.

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;

// Anchor symbol so downstream crates can chain `#[used]` statics to
// keep this rlib in the link graph. Without an explicit reference,
// rustc elides the rlib (it's mostly extern decls + a trait impl that
// gets inlined into callers), and the build.rs `cargo:rustc-link-lib=`
// directive for `libnros_platform_posix.a` is dropped along with it,
// leaving every `nros_platform_*` symbol unresolved at the binary
// link step.
#[cfg(feature = "posix-c-port")]
#[doc(hidden)]
#[inline(never)]
pub extern "C" fn _nros_force_link_cffi() {}

// ============================================================================
// Canonical ABI declarations
// ----------------------------------------------------------------------------
// Hand-written mirror of `include/nros/platform.h`. Field order, names,
// and types track the header byte-for-byte. Updates land in the header
// first, then here.
// ============================================================================

// RFC-0054 (phase-299 W2): the extern declarations are GENERATED from the
// platform headers (src/generated.rs, scripts/gen-abi-bindings.sh) — the
// C headers are the SSoT. The nros_platform_export_*! macros below stay
// hand-written: they EMIT the definitions (the port side).
pub mod generated;
pub use generated::*;

/// Board-supplied writer fn type. ONLY meaningful on platforms whose
/// `nros_platform_log_write` impl is itself a thin dispatcher to a
/// board-registered fn (FreeRTOS, ThreadX, bare-metal). On platforms
/// with a native logger (POSIX, Zephyr, ESP-IDF, NuttX), the symbol
/// is absent and the board should not link against it.
pub type NrosPlatformLogWriterFn = unsafe extern "C" fn(
    severity: u8,
    name_ptr: *const u8,
    name_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
);

/// Board-supplied flush fn type. Pass `None` to
/// [`nros_platform_register_log_writer`] when the writer is fully
/// synchronous.
pub type NrosPlatformLogFlushFn = unsafe extern "C" fn();

// ============================================================================
// Phase 121.6.rust-mirror — extended canonical ABI
// ----------------------------------------------------------------------------
// Mirrors `<nros/platform_timer.h>` + `<nros/platform_net.h>`. Declarations
// only — definitions are supplied by whichever provider the binary links
// (a per-RTOS C port via 121.6.<port>-c, or a future macro-expanded Rust
// impl). Anyone NOT pulling these via `CffiPlatform`'s extended-surface
// trait impls (those land in a follow-up commit) gets dead-code-stripped
// extern refs at link time — no symbol resolution required.
// ============================================================================

// ============================================================================
// Return codes (mirrors header)
// ============================================================================

/// Mirrors C `nros_platform_ret_t`.
pub type NrosPlatformRet = i32;

pub const NROS_PLATFORM_RET_OK: NrosPlatformRet = 0;
pub const NROS_PLATFORM_RET_ERROR: NrosPlatformRet = -1;
pub const NROS_PLATFORM_RET_UNSUPPORTED: NrosPlatformRet = -5;

// ============================================================================
// CffiPlatform — trait impls dispatching to the linked C symbols
// ============================================================================

/// Zero-sized type implementing the platform traits via the canonical
/// `nros_platform_*` C symbols.
///
/// The crate that pulls `CffiPlatform` into a final binary is
/// responsible for ensuring the symbols are supplied at link time
/// (either by a C translation unit or a Rust `-cffi` shim crate).
pub struct CffiPlatform;

impl nros_platform_api::PlatformClock for CffiPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        unsafe { nros_platform_clock_ms() }
    }

    #[inline]
    fn clock_us() -> u64 {
        unsafe { nros_platform_clock_us() }
    }
}

impl nros_platform_api::PlatformAlloc for CffiPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        unsafe { nros_platform_alloc(size) }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        unsafe { nros_platform_realloc(ptr, size) }
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        unsafe { nros_platform_dealloc(ptr) }
    }

    #[inline]
    fn heap_used_bytes() -> usize {
        unsafe { nros_platform_heap_used_bytes() }
    }

    #[inline]
    fn heap_total_bytes() -> usize {
        unsafe { nros_platform_heap_total_bytes() }
    }
}

impl nros_platform_api::PlatformSleep for CffiPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        unsafe { nros_platform_sleep_us(us) }
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        unsafe { nros_platform_sleep_ms(ms) }
    }

    #[inline]
    fn sleep_s(s: usize) {
        unsafe { nros_platform_sleep_s(s) }
    }
}

impl nros_platform_api::PlatformYield for CffiPlatform {
    #[inline]
    fn yield_now() {
        unsafe { nros_platform_yield_now() }
    }
}

// Phase 110.D — `PlatformScheduler` is satisfied by the existing yield
// symbol; per-thread scheduling controls land when a C consumer needs
// hard-RT preemption.
impl nros_platform_api::PlatformScheduler for CffiPlatform {
    #[inline]
    fn yield_now() {
        unsafe { nros_platform_yield_now() }
    }
}

// Phase 110.E.b — `PlatformTimer` dispatches to the
// `nros_platform_timer_*` C ABI declared above. Backed by
// `nros-platform-posix/src/timer.c` on POSIX (POSIX `timer_create` +
// `SIGEV_THREAD` trampoline); each RTOS port supplies its own
// `timer.c` mirroring the canonical signatures.
//
// `TimerHandle` is a `*mut c_void` newtype so the trait's
// `Send + Sync` bound holds. Safety: the C layer owns the heap
// record behind the pointer; Rust just shuttles the opaque handle
// between `create_*` and `destroy` / `cancel`.
#[derive(Debug)]
pub struct CffiTimerHandle(*mut c_void);

// SAFETY: the underlying `*mut c_void` is an opaque platform-owned
// handle (POSIX `timer_t` wrapped in a heap record, FreeRTOS
// `TimerHandle_t`, etc.). The Rust side never dereferences it; the
// only operations are forwarding it back to `destroy` / `cancel`.
// Send + Sync are required by the trait so the executor can stash
// the handle across thread boundaries.
unsafe impl Send for CffiTimerHandle {}
unsafe impl Sync for CffiTimerHandle {}

impl nros_platform_api::PlatformTimer for CffiPlatform {
    type TimerHandle = CffiTimerHandle;

    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        // `extern "C" fn` coerces structurally to `unsafe extern "C"
        // fn` — both have the same ABI; Rust just demands the unsafe
        // version at the C call site.
        let cb: unsafe extern "C" fn(*mut c_void) = callback;
        let raw = unsafe { nros_platform_timer_create_periodic(period_us, Some(cb), user_data) };
        if raw.is_null() {
            // The C layer returns NULL for both "unsupported on this
            // platform" (default stub) and "syscall failed" (POSIX
            // EINVAL / kernel error). The runtime treats both the
            // same way (drop back to the polled-clock fallback), so
            // surface `KernelError` to differentiate from the
            // trait-default `Unsupported` that fires when the C
            // symbol isn't linked at all.
            return Err(nros_platform_api::TimerError::KernelError);
        }
        Ok(CffiTimerHandle(raw))
    }

    fn create_oneshot(
        timeout_us: u32,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        let cb: unsafe extern "C" fn(*mut c_void) = callback;
        let raw = unsafe { nros_platform_timer_create_oneshot(timeout_us, Some(cb), user_data) };
        if raw.is_null() {
            return Err(nros_platform_api::TimerError::KernelError);
        }
        Ok(CffiTimerHandle(raw))
    }

    fn destroy(handle: Self::TimerHandle) {
        unsafe { nros_platform_timer_destroy(handle.0) }
    }

    fn cancel(handle: &mut Self::TimerHandle) -> bool {
        let rc = unsafe { nros_platform_timer_cancel(handle.0) };
        // `1` = cancellation prevented the callback from firing;
        // `0` / `-1` = already fired (or error — treated as "not
        // cancelled in time" by the caller).
        rc == 1
    }
}

impl nros_platform_api::PlatformRandom for CffiPlatform {
    #[inline]
    fn random_u8() -> u8 {
        unsafe { nros_platform_random_u8() }
    }

    #[inline]
    fn random_u16() -> u16 {
        unsafe { nros_platform_random_u16() }
    }

    #[inline]
    fn random_u32() -> u32 {
        unsafe { nros_platform_random_u32() }
    }

    #[inline]
    fn random_u64() -> u64 {
        unsafe { nros_platform_random_u64() }
    }

    #[inline]
    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe { nros_platform_random_fill(buf, len) }
    }
}

impl nros_platform_api::PlatformTime for CffiPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        unsafe { nros_platform_time_now_ms() }
    }

    #[inline]
    fn time_since_epoch_secs() -> u32 {
        unsafe { nros_platform_time_since_epoch_secs() }
    }

    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        unsafe { nros_platform_time_since_epoch_nanos() }
    }
}

impl nros_platform_api::PlatformThreading for CffiPlatform {
    fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        unsafe { nros_platform_task_init(task, attr, entry, arg) }
    }
    fn task_join(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_join(task) }
    }
    fn task_detach(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_detach(task) }
    }
    fn task_cancel(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_cancel(task) }
    }
    fn task_exit() {
        unsafe { nros_platform_task_exit() }
    }
    fn task_free(task: *mut *mut c_void) {
        unsafe { nros_platform_task_free(task) }
    }
    fn mutex_init(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_init(m) }
    }
    fn mutex_drop(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_drop(m) }
    }
    fn mutex_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_lock(m) }
    }
    fn mutex_try_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_try_lock(m) }
    }
    fn mutex_unlock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_unlock(m) }
    }
    fn mutex_rec_init(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_init(m) }
    }
    fn mutex_rec_drop(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_drop(m) }
    }
    fn mutex_rec_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_lock(m) }
    }
    fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_try_lock(m) }
    }
    fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_unlock(m) }
    }
    fn condvar_init(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_init(cv) }
    }
    fn condvar_drop(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_drop(cv) }
    }
    fn condvar_signal(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_signal(cv) }
    }
    fn condvar_signal_all(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_signal_all(cv) }
    }
    fn condvar_signal_from_isr(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_signal_from_isr(cv) }
    }
    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_wait(cv, m) }
    }
    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        unsafe { nros_platform_condvar_wait_until(cv, m, abstime) }
    }
    fn wake_init(w: *mut c_void) -> i8 {
        unsafe { nros_platform_wake_init(w) }
    }
    fn wake_drop(w: *mut c_void) -> i8 {
        unsafe { nros_platform_wake_drop(w) }
    }
    fn wake_wait_ms(w: *mut c_void, timeout_ms: u32) -> i8 {
        unsafe { nros_platform_wake_wait_ms(w, timeout_ms) }
    }
    fn wake_signal(w: *mut c_void) -> i8 {
        unsafe { nros_platform_wake_signal(w) }
    }
    fn wake_signal_from_isr(w: *mut c_void) -> i8 {
        unsafe { nros_platform_wake_signal_from_isr(w) }
    }
    fn wake_storage_size() -> usize {
        unsafe { nros_platform_wake_storage_size() }
    }
    fn wake_storage_align() -> usize {
        unsafe { nros_platform_wake_storage_align() }
    }
}

// ============================================================================
// Phase 121.3.deprecate-rust-migrate — extended-surface trait impls
// ----------------------------------------------------------------------------
// CffiPlatform dispatches PlatformTcp / PlatformUdp / PlatformUdpMulticast /
// PlatformSocketHelpers / PlatformNetworkPoll trait calls through the
// `unsafe extern "C"` declarations above. Whichever provider supplies the
// matching symbol set (a per-RTOS Rust crate with `cffi-export` on, or a
// hand-written C port) backs the dispatch transparently.
// ============================================================================

impl nros_platform_api::PlatformTcp for CffiPlatform {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        unsafe { nros_platform_tcp_create_endpoint(ep, address, port) }
    }
    fn free_endpoint(ep: *mut c_void) {
        unsafe { nros_platform_tcp_free_endpoint(ep) }
    }
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        unsafe { nros_platform_tcp_open(sock, endpoint, timeout_ms) }
    }
    fn listen(sock: *mut c_void, endpoint: *const c_void) -> i8 {
        unsafe { nros_platform_tcp_listen(sock, endpoint) }
    }
    fn close(sock: *mut c_void) {
        unsafe { nros_platform_tcp_close(sock) }
    }
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        unsafe { nros_platform_tcp_read(sock, buf, len) }
    }
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        unsafe { nros_platform_tcp_read_exact(sock, buf, len) }
    }
    fn send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        unsafe { nros_platform_tcp_send(sock, buf, len) }
    }
}

impl nros_platform_api::PlatformUdp for CffiPlatform {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8 {
        unsafe { nros_platform_udp_create_endpoint(ep, address, port) }
    }
    fn free_endpoint(ep: *mut c_void) {
        unsafe { nros_platform_udp_free_endpoint(ep) }
    }
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        unsafe { nros_platform_udp_open(sock, endpoint, timeout_ms) }
    }
    fn listen(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        unsafe { nros_platform_udp_listen(sock, endpoint, timeout_ms) }
    }
    fn close(sock: *mut c_void) {
        unsafe { nros_platform_udp_close(sock) }
    }
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        unsafe { nros_platform_udp_read(sock, buf, len) }
    }
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        unsafe { nros_platform_udp_read_exact(sock, buf, len) }
    }
    fn send(sock: *const c_void, buf: *const u8, len: usize, endpoint: *const c_void) -> usize {
        unsafe { nros_platform_udp_send(sock, buf, len, endpoint) }
    }
    fn set_recv_timeout(sock: *const c_void, timeout_ms: u32) {
        unsafe { nros_platform_udp_set_recv_timeout(sock, timeout_ms) }
    }
}

impl nros_platform_api::PlatformUdpMulticast for CffiPlatform {
    fn mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        iface: *const u8,
    ) -> i8 {
        unsafe { nros_platform_udp_mcast_open(sock, endpoint, lep, timeout_ms, iface) }
    }
    fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        iface: *const u8,
        join: *const u8,
    ) -> i8 {
        unsafe { nros_platform_udp_mcast_listen(sock, endpoint, timeout_ms, iface, join) }
    }
    fn mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        rep: *const c_void,
        lep: *const c_void,
    ) {
        unsafe { nros_platform_udp_mcast_close(sockrecv, socksend, rep, lep) }
    }
    fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        unsafe { nros_platform_udp_mcast_read(sock, buf, len, lep, addr) }
    }
    fn mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize {
        unsafe { nros_platform_udp_mcast_read_exact(sock, buf, len, lep, addr) }
    }
    fn mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize {
        unsafe { nros_platform_udp_mcast_send(sock, buf, len, endpoint) }
    }
}

impl nros_platform_api::PlatformSocketHelpers for CffiPlatform {
    fn set_non_blocking(sock: *const c_void) -> i8 {
        unsafe { nros_platform_socket_set_non_blocking(sock) }
    }
    fn accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8 {
        unsafe { nros_platform_socket_accept(sock_in, sock_out) }
    }
    fn close(sock: *mut c_void) {
        unsafe { nros_platform_socket_close(sock) }
    }
    fn wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8 {
        unsafe { nros_platform_socket_wait_event(peers, mutex) }
    }
}

impl nros_platform_api::PlatformNetworkPoll for CffiPlatform {
    fn network_poll() {
        unsafe { nros_platform_network_poll() }
    }
}

impl nros_platform_api::PlatformCriticalSection for CffiPlatform {
    fn acquire() -> u32 {
        unsafe { nros_platform_critical_section_acquire() }
    }
    fn release(token: u32) {
        unsafe { nros_platform_critical_section_release(token) }
    }
}

impl nros_platform_api::PlatformLog for CffiPlatform {
    fn write(severity: u8, name: &[u8], message: &[u8]) {
        // SAFETY: extern decl matches the C ABI byte-for-byte; the
        // pointer/length pairs come from `&[u8]` references that
        // outlive the call.
        unsafe {
            nros_platform_log_write(
                severity,
                name.as_ptr(),
                name.len(),
                message.as_ptr(),
                message.len(),
            );
        }
    }

    fn flush() {
        // SAFETY: no args, no preconditions.
        unsafe { nros_platform_log_flush() };
    }
}

// ============================================================================
// Phase 121.2 — export_*! macros
// ----------------------------------------------------------------------------
// Each macro emits the `#[unsafe(no_mangle)] extern "C"` definitions for one
// capability group. The macro callee must implement the matching
// `nros_platform_api::Platform*` trait; the trait bound is checked at the
// macro-expansion site, so a missing impl produces a clear compile error in
// the caller crate.
//
// Naming the symbols exactly matches `<nros/platform.h>`. Add a new ABI
// symbol in three coordinated places, all inside this crate:
//   1. declare it in `include/nros/platform.h`,
//   2. declare it in the `unsafe extern "C" { … }` block above,
//   3. emit it from the appropriate `export_*!` macro below.
// ============================================================================

/// Emit `nros_platform_clock_{ms,us}` delegating to
/// `<$ty as PlatformClock>`.
#[macro_export]
macro_rules! nros_platform_export_clock {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_clock_ms() -> u64 {
            <$ty as ::nros_platform_api::PlatformClock>::clock_ms()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_clock_us() -> u64 {
            <$ty as ::nros_platform_api::PlatformClock>::clock_us()
        }
    };
}

/// Emit `nros_platform_{alloc,realloc,dealloc}` delegating to
/// `<$ty as PlatformAlloc>`.
#[macro_export]
macro_rules! nros_platform_export_alloc {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_alloc(size: usize) -> *mut ::core::ffi::c_void {
            <$ty as ::nros_platform_api::PlatformAlloc>::alloc(size)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_realloc(
            ptr: *mut ::core::ffi::c_void,
            size: usize,
        ) -> *mut ::core::ffi::c_void {
            <$ty as ::nros_platform_api::PlatformAlloc>::realloc(ptr, size)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_dealloc(ptr: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformAlloc>::dealloc(ptr)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_heap_used_bytes() -> usize {
            <$ty as ::nros_platform_api::PlatformAlloc>::heap_used_bytes()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_heap_total_bytes() -> usize {
            <$ty as ::nros_platform_api::PlatformAlloc>::heap_total_bytes()
        }
    };
}

/// Emit `nros_platform_sleep_{us,ms,s}` delegating to
/// `<$ty as PlatformSleep>`.
#[macro_export]
macro_rules! nros_platform_export_sleep {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_sleep_us(us: usize) {
            <$ty as ::nros_platform_api::PlatformSleep>::sleep_us(us)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_sleep_ms(ms: usize) {
            <$ty as ::nros_platform_api::PlatformSleep>::sleep_ms(ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_sleep_s(s: usize) {
            <$ty as ::nros_platform_api::PlatformSleep>::sleep_s(s)
        }
    };
}

/// Emit `nros_platform_yield_now` delegating to
/// `<$ty as PlatformYield>`.
#[macro_export]
macro_rules! nros_platform_export_yield {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_yield_now() {
            <$ty as ::nros_platform_api::PlatformYield>::yield_now()
        }
    };
}

/// Emit `nros_platform_random_*` delegating to `<$ty as PlatformRandom>`.
#[macro_export]
macro_rules! nros_platform_export_random {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_random_u8() -> u8 {
            <$ty as ::nros_platform_api::PlatformRandom>::random_u8()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_random_u16() -> u16 {
            <$ty as ::nros_platform_api::PlatformRandom>::random_u16()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_random_u32() -> u32 {
            <$ty as ::nros_platform_api::PlatformRandom>::random_u32()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_random_u64() -> u64 {
            <$ty as ::nros_platform_api::PlatformRandom>::random_u64()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_random_fill(buf: *mut ::core::ffi::c_void, len: usize) {
            <$ty as ::nros_platform_api::PlatformRandom>::random_fill(buf, len)
        }
    };
}

/// Emit `nros_platform_time_*` delegating to `<$ty as PlatformTime>`.
#[macro_export]
macro_rules! nros_platform_export_time {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_time_now_ms() -> u64 {
            <$ty as ::nros_platform_api::PlatformTime>::time_now_ms()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_time_since_epoch_secs() -> u32 {
            <$ty as ::nros_platform_api::PlatformTime>::time_since_epoch_secs()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_time_since_epoch_nanos() -> u32 {
            <$ty as ::nros_platform_api::PlatformTime>::time_since_epoch_nanos()
        }
    };
}

/// Emit `nros_platform_task_*`, `nros_platform_mutex_*`,
/// `nros_platform_mutex_rec_*`, and `nros_platform_condvar_*` delegating
/// to `<$ty as PlatformThreading>`. Skip this macro on platforms without
/// kernel threads.
#[macro_export]
macro_rules! nros_platform_export_threading {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_init(
            task: *mut ::core::ffi::c_void,
            attr: *mut ::core::ffi::c_void,
            entry: ::core::option::Option<
                unsafe extern "C" fn(*mut ::core::ffi::c_void) -> *mut ::core::ffi::c_void,
            >,
            arg: *mut ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::task_init(task, attr, entry, arg)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_join(task: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::task_join(task)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_detach(task: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::task_detach(task)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_cancel(task: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::task_cancel(task)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_exit() {
            <$ty as ::nros_platform_api::PlatformThreading>::task_exit()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_task_free(task: *mut *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformThreading>::task_free(task)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_init(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_init(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_drop(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_drop(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_lock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_lock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_try_lock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_try_lock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_unlock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_unlock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_rec_init(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_rec_init(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_rec_drop(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_rec_drop(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_rec_lock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_rec_lock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_rec_try_lock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_rec_try_lock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_mutex_rec_unlock(m: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::mutex_rec_unlock(m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_init(cv: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_init(cv)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_drop(cv: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_drop(cv)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_signal(cv: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_signal(cv)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_signal_all(cv: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_signal_all(cv)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_signal_from_isr(
            cv: *mut ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_signal_from_isr(cv)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_wait(
            cv: *mut ::core::ffi::c_void,
            m: *mut ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_wait(cv, m)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_condvar_wait_until(
            cv: *mut ::core::ffi::c_void,
            m: *mut ::core::ffi::c_void,
            abstime: u64,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::condvar_wait_until(cv, m, abstime)
        }
        // Phase 130 — wake primitive (binary semaphore shape).
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_init(w: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_init(w)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_drop(w: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_drop(w)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_wait_ms(
            w: *mut ::core::ffi::c_void,
            timeout_ms: u32,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_wait_ms(w, timeout_ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_signal(w: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_signal(w)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_signal_from_isr(w: *mut ::core::ffi::c_void) -> i8 {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_signal_from_isr(w)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_storage_size() -> usize {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_storage_size()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_wake_storage_align() -> usize {
            <$ty as ::nros_platform_api::PlatformThreading>::wake_storage_align()
        }
    };
}

/// Phase 121.9 — emit the two `nros_platform_critical_section_*`
/// symbols by delegating to the caller's `PlatformCriticalSection`
/// impl.
#[macro_export]
macro_rules! nros_platform_export_critical_section {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_critical_section_acquire() -> u32 {
            <$ty as ::nros_platform_api::PlatformCriticalSection>::acquire()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_critical_section_release(token: u32) {
            <$ty as ::nros_platform_api::PlatformCriticalSection>::release(token)
        }
    };
}

/// Phase 88.11 — emit `nros_platform_log_write` + `nros_platform_log_flush`
/// from a `PlatformLog`-implementing ZST. Use this on bare-metal /
/// custom platforms (mps2-an385, stm32f4, esp32-baremetal, …) that
/// don't ship a separate C implementation file. The implementor's
/// `write` receives the rendered body + logger name as `&[u8]` slices.
#[macro_export]
macro_rules! nros_platform_export_log {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_log_write(
            severity: u8,
            name_ptr: *const u8,
            name_len: usize,
            msg_ptr: *const u8,
            msg_len: usize,
        ) {
            // SAFETY: caller passes valid `&[u8]` slices that outlive
            // the call; empty-name case (name_ptr=null, name_len=0)
            // collapses to an empty slice.
            let name: &[u8] = if name_ptr.is_null() || name_len == 0 {
                &[]
            } else {
                unsafe { ::core::slice::from_raw_parts(name_ptr, name_len) }
            };
            let msg: &[u8] = if msg_ptr.is_null() || msg_len == 0 {
                &[]
            } else {
                unsafe { ::core::slice::from_raw_parts(msg_ptr, msg_len) }
            };
            <$ty as ::nros_platform_api::PlatformLog>::write(severity, name, msg);
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_log_flush() {
            <$ty as ::nros_platform_api::PlatformLog>::flush()
        }
        /// Phase 88.16.H — ABI-mirror parity. Direct-impl
        /// platforms (`mps2-an385`, `stm32f4`, …) route every
        /// record through `PlatformLog::write`, so the runtime
        /// swap slot is meaningless to them. The header mirror
        /// nonetheless declares `nros_platform_register_log_writer`,
        /// so the macro emits a no-op stub to satisfy the
        /// ABI-mirror check. Fn-ptr-slot platforms (FreeRTOS /
        /// ThreadX / NuttX) don't call this macro — their C body
        /// ships the real strong definition.
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_register_log_writer(
            _writer: ::core::option::Option<
                unsafe extern "C" fn(
                    severity: u8,
                    name_ptr: *const u8,
                    name_len: usize,
                    msg_ptr: *const u8,
                    msg_len: usize,
                ),
            >,
            _flusher: ::core::option::Option<unsafe extern "C" fn()>,
        ) {
        }
    };
}

/// Convenience: emit every `nros_platform_*` symbol declared in
/// `<nros/platform.h>` by delegating to the corresponding
/// `nros_platform_api::Platform*` trait method on `$ty`. The caller must
/// implement every trait covered by the capability macros.
///
/// Logging (`nros_platform_export_log!`) is NOT part of this convenience
/// macro: bare-metal platforms typically need to supply a writer
/// (`hprintln!` / `defmt::info!`) that requires extra deps not all
/// platforms link against. Call `nros_platform_export_log!` separately
/// after the platform crate implements `PlatformLog`.
#[macro_export]
macro_rules! nros_platform_export {
    ($ty:ty) => {
        $crate::nros_platform_export_clock!($ty);
        $crate::nros_platform_export_alloc!($ty);
        $crate::nros_platform_export_sleep!($ty);
        $crate::nros_platform_export_yield!($ty);
        $crate::nros_platform_export_random!($ty);
        $crate::nros_platform_export_time!($ty);
        $crate::nros_platform_export_threading!($ty);
        $crate::nros_platform_export_critical_section!($ty);
    };
}

// ============================================================================
// Phase 121.6.macros — extended-surface export macros
// ----------------------------------------------------------------------------
// `nros_platform_export_net!` mirrors `<nros/platform_net.h>` 1:1; trait
// signatures match the C ABI byte-for-byte. `nros_platform_export_timer!`
// adapts the Rust `PlatformTimer` trait's `Result<TimerHandle, _>` to the
// C ABI's `*mut c_void` (NULL on error). The caller's `TimerHandle`
// associated type must be `*mut c_void` — enforced at macro-expansion
// time via a `where` clause on the emitted dispatch functions.
// ============================================================================

/// Emit every `nros_platform_timer_*` symbol declared in
/// `<nros/platform_timer.h>` by delegating to the corresponding
/// `PlatformTimer` trait method on `$ty`.
///
/// **Constraint:** the implementor's `TimerHandle` associated type
/// must be `*mut core::ffi::c_void` so the macro can pass the handle
/// through the C ABI unchanged. Implementations using kernel-specific
/// handle types should wrap them in a `*mut c_void` (typically by
/// `Box::into_raw` + a thin newtype) before exporting.
#[macro_export]
macro_rules! nros_platform_export_timer {
    ($ty:ty) => {
        // Compile-time guard: handle must be pointer-sized so the
        // transmute below is sound. PlatformTimer requires Send +
        // Sync + 'static, which `*mut c_void` itself fails — so
        // callers wrap their handle in a `#[repr(transparent)]`
        // newtype that implements those (PosixTimerHandle, etc.).
        // We round-trip through transmute at the C ABI boundary.
        const _: () = {
            if ::core::mem::size_of::<<$ty as ::nros_platform_api::PlatformTimer>::TimerHandle>()
                != ::core::mem::size_of::<*mut ::core::ffi::c_void>()
            {
                panic!(
                    "nros_platform_export_timer! requires \
                     PlatformTimer::TimerHandle to be pointer-sized"
                );
            }
        };

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_timer_create_periodic(
            period_us: u32,
            callback: extern "C" fn(*mut ::core::ffi::c_void),
            user_data: *mut ::core::ffi::c_void,
        ) -> *mut ::core::ffi::c_void {
            match <$ty as ::nros_platform_api::PlatformTimer>::create_periodic(
                period_us, callback, user_data,
            ) {
                Ok(h) => unsafe {
                    ::core::mem::transmute_copy::<
                        <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle,
                        *mut ::core::ffi::c_void,
                    >(&::core::mem::ManuallyDrop::new(h))
                },
                Err(_) => ::core::ptr::null_mut(),
            }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_timer_create_oneshot(
            timeout_us: u32,
            callback: extern "C" fn(*mut ::core::ffi::c_void),
            user_data: *mut ::core::ffi::c_void,
        ) -> *mut ::core::ffi::c_void {
            match <$ty as ::nros_platform_api::PlatformTimer>::create_oneshot(
                timeout_us, callback, user_data,
            ) {
                Ok(h) => unsafe {
                    ::core::mem::transmute_copy::<
                        <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle,
                        *mut ::core::ffi::c_void,
                    >(&::core::mem::ManuallyDrop::new(h))
                },
                Err(_) => ::core::ptr::null_mut(),
            }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_timer_destroy(handle: *mut ::core::ffi::c_void) {
            let h: <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle = unsafe {
                ::core::mem::transmute_copy::<
                    *mut ::core::ffi::c_void,
                    <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle,
                >(&handle)
            };
            <$ty as ::nros_platform_api::PlatformTimer>::destroy(h)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_timer_cancel(handle: *mut ::core::ffi::c_void) -> i8 {
            let mut h: <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle = unsafe {
                ::core::mem::transmute_copy::<
                    *mut ::core::ffi::c_void,
                    <$ty as ::nros_platform_api::PlatformTimer>::TimerHandle,
                >(&handle)
            };
            if <$ty as ::nros_platform_api::PlatformTimer>::cancel(&mut h) {
                1
            } else {
                0
            }
        }
    };
}

/// Emit every `nros_platform_tcp_*` / `nros_platform_udp_*` /
/// `nros_platform_udp_mcast_*` / `nros_platform_socket_*` /
/// `nros_platform_network_poll` symbol declared in
/// `<nros/platform_net.h>` by delegating to the corresponding trait
/// method on `$ty`. The caller must implement `PlatformTcp`,
/// `PlatformUdp`, `PlatformUdpMulticast`, `PlatformSocketHelpers`, and
/// `PlatformNetworkPoll`.
#[macro_export]
macro_rules! nros_platform_export_net {
    ($ty:ty) => {
        // ---- TCP ----
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_create_endpoint(
            ep: *mut ::core::ffi::c_void,
            address: *const u8,
            port: *const u8,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformTcp>::create_endpoint(ep, address, port)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_free_endpoint(ep: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformTcp>::free_endpoint(ep)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_open(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
            timeout_ms: u32,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformTcp>::open(sock, endpoint, timeout_ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_listen(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformTcp>::listen(sock, endpoint)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_close(sock: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformTcp>::close(sock)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_read(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformTcp>::read(sock, buf, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_read_exact(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformTcp>::read_exact(sock, buf, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_tcp_send(
            sock: *const ::core::ffi::c_void,
            buf: *const u8,
            len: usize,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformTcp>::send(sock, buf, len)
        }

        // ---- UDP unicast ----
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_create_endpoint(
            ep: *mut ::core::ffi::c_void,
            address: *const u8,
            port: *const u8,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformUdp>::create_endpoint(ep, address, port)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_free_endpoint(ep: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformUdp>::free_endpoint(ep)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_open(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
            timeout_ms: u32,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformUdp>::open(sock, endpoint, timeout_ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_listen(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
            timeout_ms: u32,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformUdp>::listen(sock, endpoint, timeout_ms)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_close(sock: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformUdp>::close(sock)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_read(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdp>::read(sock, buf, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_read_exact(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdp>::read_exact(sock, buf, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_send(
            sock: *const ::core::ffi::c_void,
            buf: *const u8,
            len: usize,
            endpoint: *const ::core::ffi::c_void,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdp>::send(sock, buf, len, endpoint)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_set_recv_timeout(
            sock: *const ::core::ffi::c_void,
            timeout_ms: u32,
        ) {
            <$ty as ::nros_platform_api::PlatformUdp>::set_recv_timeout(sock, timeout_ms)
        }

        // ---- UDP multicast ----
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_open(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
            lep: *mut ::core::ffi::c_void,
            timeout_ms: u32,
            iface: *const u8,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_open(
                sock, endpoint, lep, timeout_ms, iface,
            )
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_listen(
            sock: *mut ::core::ffi::c_void,
            endpoint: *const ::core::ffi::c_void,
            timeout_ms: u32,
            iface: *const u8,
            join: *const u8,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_listen(
                sock, endpoint, timeout_ms, iface, join,
            )
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_close(
            sockrecv: *mut ::core::ffi::c_void,
            socksend: *mut ::core::ffi::c_void,
            rep: *const ::core::ffi::c_void,
            lep: *const ::core::ffi::c_void,
        ) {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_close(
                sockrecv, socksend, rep, lep,
            )
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_read(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
            lep: *const ::core::ffi::c_void,
            addr: *mut ::core::ffi::c_void,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_read(
                sock, buf, len, lep, addr,
            )
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_read_exact(
            sock: *const ::core::ffi::c_void,
            buf: *mut u8,
            len: usize,
            lep: *const ::core::ffi::c_void,
            addr: *mut ::core::ffi::c_void,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_read_exact(
                sock, buf, len, lep, addr,
            )
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_udp_mcast_send(
            sock: *const ::core::ffi::c_void,
            buf: *const u8,
            len: usize,
            endpoint: *const ::core::ffi::c_void,
        ) -> usize {
            <$ty as ::nros_platform_api::PlatformUdpMulticast>::mcast_send(sock, buf, len, endpoint)
        }

        // ---- Socket helpers ----
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_socket_set_non_blocking(
            sock: *const ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformSocketHelpers>::set_non_blocking(sock)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_socket_accept(
            sock_in: *const ::core::ffi::c_void,
            sock_out: *mut ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformSocketHelpers>::accept(sock_in, sock_out)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_socket_close(sock: *mut ::core::ffi::c_void) {
            <$ty as ::nros_platform_api::PlatformSocketHelpers>::close(sock)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_socket_wait_event(
            peers: *mut ::core::ffi::c_void,
            mutex: *mut ::core::ffi::c_void,
        ) -> i8 {
            <$ty as ::nros_platform_api::PlatformSocketHelpers>::wait_event(peers, mutex)
        }

        // ---- Network poll ----
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_network_poll() {
            <$ty as ::nros_platform_api::PlatformNetworkPoll>::network_poll()
        }
    };
}

// ============================================================================
// Test-only self-export
// ----------------------------------------------------------------------------
// `cargo test -p nros-platform-cffi` builds a test binary that links the
// rlib. The `unsafe extern "C"` declarations above would fail to link
// without definitions; we satisfy them by invoking the macro on a dummy
// `TestPlatform` ZST defined here. This doubles as a smoke test that
// every macro arm expands and that the trait dispatch resolves.
//
// Real platform crates supply their own definitions via the same macro
// and never compile this module (it is gated on `cfg(test)`).
// ============================================================================

#[cfg(all(test, not(feature = "c-stub-test"), not(feature = "posix-c-port")))]
mod test_self_export {
    use core::ffi::c_void;
    use nros_platform_api::{
        PlatformAlloc, PlatformClock, PlatformRandom, PlatformSleep, PlatformThreading,
        PlatformTime, PlatformYield,
    };

    pub struct TestPlatform;

    impl PlatformClock for TestPlatform {
        fn clock_ms() -> u64 {
            0
        }
        fn clock_us() -> u64 {
            0
        }
    }
    impl PlatformAlloc for TestPlatform {
        fn alloc(_: usize) -> *mut c_void {
            core::ptr::null_mut()
        }
        fn realloc(_: *mut c_void, _: usize) -> *mut c_void {
            core::ptr::null_mut()
        }
        fn dealloc(_: *mut c_void) {}
    }
    impl PlatformSleep for TestPlatform {
        fn sleep_us(_: usize) {}
        fn sleep_ms(_: usize) {}
        fn sleep_s(_: usize) {}
    }
    impl PlatformYield for TestPlatform {
        fn yield_now() {}
    }
    impl PlatformRandom for TestPlatform {
        fn random_u8() -> u8 {
            0
        }
        fn random_u16() -> u16 {
            0
        }
        fn random_u32() -> u32 {
            0
        }
        fn random_u64() -> u64 {
            0
        }
        fn random_fill(_: *mut c_void, _: usize) {}
    }
    impl PlatformTime for TestPlatform {
        fn time_now_ms() -> u64 {
            0
        }
        fn time_since_epoch_secs() -> u32 {
            0
        }
        fn time_since_epoch_nanos() -> u32 {
            0
        }
    }
    impl PlatformThreading for TestPlatform {
        fn task_init(
            _: *mut c_void,
            _: *mut c_void,
            _: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
            _: *mut c_void,
        ) -> i8 {
            -1
        }
        fn task_join(_: *mut c_void) -> i8 {
            -1
        }
        fn task_detach(_: *mut c_void) -> i8 {
            -1
        }
        fn task_cancel(_: *mut c_void) -> i8 {
            -1
        }
        fn task_exit() {}
        fn task_free(_: *mut *mut c_void) {}
        fn mutex_init(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_drop(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_lock(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_try_lock(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_unlock(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_rec_init(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_rec_drop(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_rec_lock(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_rec_try_lock(_: *mut c_void) -> i8 {
            0
        }
        fn mutex_rec_unlock(_: *mut c_void) -> i8 {
            0
        }
        fn condvar_init(_: *mut c_void) -> i8 {
            0
        }
        fn condvar_drop(_: *mut c_void) -> i8 {
            0
        }
        fn condvar_signal(_: *mut c_void) -> i8 {
            0
        }
        fn condvar_signal_all(_: *mut c_void) -> i8 {
            0
        }
        fn condvar_wait(_: *mut c_void, _: *mut c_void) -> i8 {
            0
        }
        fn condvar_wait_until(_: *mut c_void, _: *mut c_void, _: u64) -> i8 {
            0
        }
    }
    impl ::nros_platform_api::PlatformCriticalSection for TestPlatform {
        fn acquire() -> u32 {
            0
        }
        fn release(_: u32) {}
    }

    /// Pointer-sized newtype wrapping `*mut c_void` so the
    /// PlatformTimer Send + Sync + 'static bound is satisfied.
    /// The transmute inside `nros_platform_export_timer!` rests on
    /// this being `#[repr(transparent)]` over a pointer.
    #[repr(transparent)]
    #[derive(Clone, Copy)]
    pub struct TestTimerHandle(pub *mut c_void);
    unsafe impl Send for TestTimerHandle {}
    unsafe impl Sync for TestTimerHandle {}

    impl ::nros_platform_api::PlatformTimer for TestPlatform {
        type TimerHandle = TestTimerHandle;
        // create_periodic / create_oneshot / destroy / cancel inherit
        // the trait's default impls (return TimerError::Unsupported /
        // no-op destroy / false cancel) — fine for export-emission
        // verification.
    }

    crate::nros_platform_export!(TestPlatform);
    crate::nros_platform_export_timer!(TestPlatform);

    #[test]
    fn macro_expansion_dispatches() {
        // Touch every group through the FFI surface to confirm the
        // generated symbols are reachable and dispatch resolves.
        assert_eq!(super::CffiPlatform::clock_ms(), 0);
        assert_eq!(
            <super::CffiPlatform as ::nros_platform_api::PlatformAlloc>::alloc(0),
            core::ptr::null_mut(),
        );
        <super::CffiPlatform as ::nros_platform_api::PlatformYield>::yield_now();
    }

    #[test]
    fn timer_macro_emits() {
        // Default impl returns Unsupported → null handle.
        let h = unsafe {
            super::nros_platform_timer_create_periodic(1000, noop_callback, core::ptr::null_mut())
        };
        assert!(h.is_null(), "default Unsupported impl must surface as NULL");
    }

    extern "C" fn noop_callback(_: *mut c_void) {}
}

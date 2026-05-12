//! Phase 121.4.c + 121.6.macros — confirm that
//! `nros_platform_cffi::nros_platform_export!` (core 39) and
//! `nros_platform_cffi::nros_platform_export_net!` (28 net symbols)
//! together emit every name declared in `<nros/platform.h>` and
//! `<nros/platform_net.h>` when invoked against `PosixPlatform`.
//!
//! Gated behind the `cffi-export` feature — without it neither macro
//! is invoked.

#![cfg(feature = "cffi-export")]

use core::ffi::c_void;

// Pulls the crate's compilation units (and their `#[no_mangle]`
// items) into the test binary. Without this the integration-test
// linker doesn't see any reason to load the rlib's object files
// and the `extern "C"` references below stay unresolved.
#[allow(unused_imports)]
use nros_platform_posix::PosixPlatform;

// Mirror of `<nros/platform.h>`. If the platform crate's macro fails
// to emit any name below, this test won't link.
unsafe extern "C" {
    fn nros_platform_clock_ms() -> u64;
    fn nros_platform_clock_us() -> u64;
    fn nros_platform_alloc(size: usize) -> *mut c_void;
    fn nros_platform_realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn nros_platform_dealloc(ptr: *mut c_void);
    fn nros_platform_sleep_us(us: usize);
    fn nros_platform_sleep_ms(ms: usize);
    fn nros_platform_sleep_s(s: usize);
    fn nros_platform_yield_now();
    fn nros_platform_random_u8() -> u8;
    fn nros_platform_random_u16() -> u16;
    fn nros_platform_random_u32() -> u32;
    fn nros_platform_random_u64() -> u64;
    fn nros_platform_random_fill(buf: *mut c_void, len: usize);
    fn nros_platform_time_now_ms() -> u64;
    fn nros_platform_time_since_epoch_secs() -> u32;
    fn nros_platform_time_since_epoch_nanos() -> u32;
    fn nros_platform_task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8;
    fn nros_platform_task_join(task: *mut c_void) -> i8;
    fn nros_platform_task_detach(task: *mut c_void) -> i8;
    fn nros_platform_task_cancel(task: *mut c_void) -> i8;
    fn nros_platform_task_exit();
    fn nros_platform_task_free(task: *mut *mut c_void);
    fn nros_platform_mutex_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_try_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_unlock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_try_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_unlock(m: *mut c_void) -> i8;
    fn nros_platform_condvar_init(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_drop(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_signal(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_signal_all(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;
    fn nros_platform_condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8;

    // Mirror of `<nros/platform_net.h>` (emitted by
    // `nros_platform_export_net!` in posix's lib.rs).
    fn nros_platform_tcp_create_endpoint(
        ep: *mut c_void,
        address: *const u8,
        port: *const u8,
    ) -> i8;
    fn nros_platform_tcp_free_endpoint(ep: *mut c_void);
    fn nros_platform_tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8;
    fn nros_platform_tcp_listen(sock: *mut c_void, endpoint: *const c_void) -> i8;
    fn nros_platform_tcp_close(sock: *mut c_void);
    fn nros_platform_tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_tcp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize;
    fn nros_platform_udp_create_endpoint(
        ep: *mut c_void,
        address: *const u8,
        port: *const u8,
    ) -> i8;
    fn nros_platform_udp_free_endpoint(ep: *mut c_void);
    fn nros_platform_udp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8;
    fn nros_platform_udp_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
    ) -> i8;
    fn nros_platform_udp_close(sock: *mut c_void);
    fn nros_platform_udp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_udp_read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn nros_platform_udp_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize;
    fn nros_platform_udp_set_recv_timeout(sock: *const c_void, timeout_ms: u32);
    fn nros_platform_udp_mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        iface: *const u8,
    ) -> i8;
    fn nros_platform_udp_mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        iface: *const u8,
        join: *const u8,
    ) -> i8;
    fn nros_platform_udp_mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        rep: *const c_void,
        lep: *const c_void,
    );
    fn nros_platform_udp_mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize;
    fn nros_platform_udp_mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize;
    fn nros_platform_udp_mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize;
    fn nros_platform_socket_set_non_blocking(sock: *const c_void) -> i8;
    fn nros_platform_socket_accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8;
    fn nros_platform_socket_close(sock: *mut c_void);
    fn nros_platform_socket_wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8;
    fn nros_platform_network_poll();
}

#[test]
fn posix_macro_emits_every_symbol() {
    // Cheap dispatch — the value of the test is in linking, not in
    // exercising POSIX behaviour. (PosixPlatform's own crate tests
    // cover that.)
    let t0 = unsafe { nros_platform_clock_ms() };
    std::thread::sleep(std::time::Duration::from_millis(2));
    let t1 = unsafe { nros_platform_clock_ms() };
    assert!(t1 >= t0, "clock_ms must be monotonic via cffi export");

    let _ = unsafe { nros_platform_clock_us() };
    let _ = unsafe { nros_platform_time_now_ms() };
    let _ = unsafe { nros_platform_time_since_epoch_secs() };
    let _ = unsafe { nros_platform_time_since_epoch_nanos() };
    let _ = unsafe { nros_platform_random_u32() };
    let _ = unsafe { nros_platform_random_u64() };
    unsafe { nros_platform_yield_now() };

    // Just touch every other symbol as a fn pointer so the linker
    // keeps them — these calls aren't safe to actually invoke under
    // libc, but `as *const ()` is sound and pins the externs.
    let pins: [*const (); 59] = [
        nros_platform_alloc as *const (),
        nros_platform_realloc as *const (),
        nros_platform_dealloc as *const (),
        nros_platform_sleep_us as *const (),
        nros_platform_sleep_ms as *const (),
        nros_platform_sleep_s as *const (),
        nros_platform_random_u8 as *const (),
        nros_platform_random_u16 as *const (),
        nros_platform_random_fill as *const (),
        nros_platform_task_init as *const (),
        nros_platform_task_join as *const (),
        nros_platform_task_detach as *const (),
        nros_platform_task_cancel as *const (),
        nros_platform_task_exit as *const (),
        nros_platform_task_free as *const (),
        nros_platform_mutex_init as *const (),
        nros_platform_mutex_drop as *const (),
        nros_platform_mutex_lock as *const (),
        nros_platform_mutex_try_lock as *const (),
        nros_platform_mutex_unlock as *const (),
        nros_platform_mutex_rec_init as *const (),
        nros_platform_mutex_rec_drop as *const (),
        nros_platform_mutex_rec_lock as *const (),
        nros_platform_mutex_rec_try_lock as *const (),
        nros_platform_mutex_rec_unlock as *const (),
        nros_platform_condvar_init as *const (),
        nros_platform_condvar_drop as *const (),
        nros_platform_condvar_signal as *const (),
        nros_platform_condvar_signal_all as *const (),
        nros_platform_condvar_wait as *const (),
        nros_platform_condvar_wait_until as *const (),
        // 121.6.macros — 28 net symbols
        nros_platform_tcp_create_endpoint as *const (),
        nros_platform_tcp_free_endpoint as *const (),
        nros_platform_tcp_open as *const (),
        nros_platform_tcp_listen as *const (),
        nros_platform_tcp_close as *const (),
        nros_platform_tcp_read as *const (),
        nros_platform_tcp_read_exact as *const (),
        nros_platform_tcp_send as *const (),
        nros_platform_udp_create_endpoint as *const (),
        nros_platform_udp_free_endpoint as *const (),
        nros_platform_udp_open as *const (),
        nros_platform_udp_listen as *const (),
        nros_platform_udp_close as *const (),
        nros_platform_udp_read as *const (),
        nros_platform_udp_read_exact as *const (),
        nros_platform_udp_send as *const (),
        nros_platform_udp_set_recv_timeout as *const (),
        nros_platform_udp_mcast_open as *const (),
        nros_platform_udp_mcast_listen as *const (),
        nros_platform_udp_mcast_close as *const (),
        nros_platform_udp_mcast_read as *const (),
        nros_platform_udp_mcast_read_exact as *const (),
        nros_platform_udp_mcast_send as *const (),
        nros_platform_socket_set_non_blocking as *const (),
        nros_platform_socket_accept as *const (),
        nros_platform_socket_close as *const (),
        nros_platform_socket_wait_event as *const (),
        nros_platform_network_poll as *const (),
    ];
    for p in pins {
        assert!(!p.is_null(), "every exported symbol must resolve");
    }
}

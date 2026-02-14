//! Time functions for zenoh-pico (system time)
//!
//! For embedded without RTC, uses the monotonic clock as system time.

use crate::clock;
use core::ffi::{c_char, c_ulong};

/// z_time_t z_time_now(void)
#[unsafe(no_mangle)]
pub extern "C" fn z_time_now() -> u64 {
    clock::clock_ms()
}

/// const char *z_time_now_as_str(char *const buf, unsigned long buflen)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_time_now_as_str(buf: *mut c_char, buflen: c_ulong) -> *const c_char {
    if buf.is_null() || buflen == 0 {
        // Return pointer to static empty string
        return c"".as_ptr();
    }

    let mut now = clock::clock_ms();
    let buflen = buflen as usize;

    unsafe {
        // Write null terminator at end
        let mut ptr = buflen - 1;
        *buf.add(ptr) = 0;

        // Simple itoa: write digits right-to-left
        loop {
            if ptr == 0 {
                break;
            }
            ptr -= 1;
            *buf.add(ptr) = b'0' as c_char + (now % 10) as c_char;
            now /= 10;
            if now == 0 {
                break;
            }
        }

        buf.add(ptr)
    }
}

/// unsigned long z_time_elapsed_us(z_time_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_time_elapsed_us(time: *const u64) -> c_ulong {
    let start = unsafe { *time };
    let elapsed_ms = clock::clock_ms().wrapping_sub(start);
    (elapsed_ms * 1000) as c_ulong
}

/// unsigned long z_time_elapsed_ms(z_time_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_time_elapsed_ms(time: *const u64) -> c_ulong {
    let start = unsafe { *time };
    clock::clock_ms().wrapping_sub(start) as c_ulong
}

/// unsigned long z_time_elapsed_s(z_time_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_time_elapsed_s(time: *const u64) -> c_ulong {
    let start = unsafe { *time };
    let elapsed_ms = clock::clock_ms().wrapping_sub(start);
    (elapsed_ms / 1000) as c_ulong
}

/// Zenoh-pico time-since-epoch struct (matches C definition)
#[repr(C)]
pub struct ZTimeSinceEpoch {
    secs: u32,
    nanos: u32,
}

/// z_result_t _z_get_time_since_epoch(_z_time_since_epoch *t)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn _z_get_time_since_epoch(t: *mut ZTimeSinceEpoch) -> i8 {
    let now_ms = clock::clock_ms();
    unsafe {
        (*t).secs = (now_ms / 1000) as u32;
        (*t).nanos = ((now_ms % 1000) * 1_000_000) as u32;
    }
    0 // _Z_RES_OK
}

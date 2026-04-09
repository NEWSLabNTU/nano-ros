//! Minimal libc stubs for bare-metal ESP32.
//!
//! ESP32 WiFi variant has esp-radio which provides most libc functions.
//! Only `strtoul` and `__errno` are needed additionally.

use core::ffi::{c_char, c_int, c_ulong};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoul(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    unsafe {
        let mut ptr = nptr;
        let mut result: c_ulong = 0;

        while *ptr as u8 == b' ' || *ptr as u8 == b'\t' {
            ptr = ptr.add(1);
        }

        let radix = if base == 0 {
            if *ptr as u8 == b'0' {
                ptr = ptr.add(1);
                if *ptr as u8 == b'x' || *ptr as u8 == b'X' {
                    ptr = ptr.add(1);
                    16
                } else {
                    8
                }
            } else {
                10
            }
        } else if base == 16 && *ptr as u8 == b'0' {
            ptr = ptr.add(1);
            if *ptr as u8 == b'x' || *ptr as u8 == b'X' {
                ptr = ptr.add(1);
            }
            16
        } else {
            base as c_ulong
        };

        loop {
            let c = *ptr as u8;
            let digit = match c {
                b'0'..=b'9' => (c - b'0') as c_ulong,
                b'a'..=b'z' => (c - b'a' + 10) as c_ulong,
                b'A'..=b'Z' => (c - b'A' + 10) as c_ulong,
                _ => break,
            };
            if digit >= radix {
                break;
            }
            result = result.wrapping_mul(radix).wrapping_add(digit);
            ptr = ptr.add(1);
        }

        if !endptr.is_null() {
            *endptr = ptr as *mut c_char;
        }
        result
    }
}

static mut ERRNO: c_int = 0;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __errno() -> *mut c_int {
    &raw mut ERRNO
}

// Alias for platforms that use `errno` directly instead of `__errno()`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn errno() -> *mut c_int {
    &raw mut ERRNO
}

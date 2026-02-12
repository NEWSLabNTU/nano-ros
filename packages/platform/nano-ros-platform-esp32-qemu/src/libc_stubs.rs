//! Minimal libc stubs for bare-metal zenoh-pico
//!
//! These functions are required by zenoh-pico but not available in no_std.
//! They provide minimal implementations sufficient for embedded use.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` FFI functions that follow C conventions.
//! Callers must ensure:
//! - All pointers are valid and properly aligned
//! - String pointers point to null-terminated C strings where expected
//! - Buffer sizes are accurate and buffers have sufficient capacity
//! - Memory regions do not overlap for memcpy (use memmove for overlapping regions)

#![allow(clippy::missing_safety_doc)]

use core::ffi::{c_char, c_int, c_ulong, c_void};

/// strlen - get string length
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    unsafe {
        let mut len = 0;
        while *s.add(len) != 0 {
            len += 1;
        }
        len
    }
}

/// memcpy - copy memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    unsafe {
        let d = dest as *mut u8;
        let s = src as *const u8;
        for i in 0..n {
            *d.add(i) = *s.add(i);
        }
        dest
    }
}

/// memmove - copy memory (handles overlapping regions)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    unsafe {
        let d = dest as *mut u8;
        let s = src as *const u8;

        if (d as usize) < (s as usize) {
            // Copy forwards
            for i in 0..n {
                *d.add(i) = *s.add(i);
            }
        } else {
            // Copy backwards
            for i in (0..n).rev() {
                *d.add(i) = *s.add(i);
            }
        }
        dest
    }
}

/// memset - fill memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut c_void, c: c_int, n: usize) -> *mut c_void {
    unsafe {
        let d = dest as *mut u8;
        for i in 0..n {
            *d.add(i) = c as u8;
        }
        dest
    }
}

/// memcmp - compare memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int {
    unsafe {
        let a = s1 as *const u8;
        let b = s2 as *const u8;
        for i in 0..n {
            let diff = (*a.add(i) as c_int) - (*b.add(i) as c_int);
            if diff != 0 {
                return diff;
            }
        }
        0
    }
}

/// memchr - find byte in memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    unsafe {
        let ptr = s as *const u8;
        let byte = c as u8;
        for i in 0..n {
            if *ptr.add(i) == byte {
                return ptr.add(i) as *mut c_void;
            }
        }
        core::ptr::null_mut()
    }
}

/// strchr - find character in string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    unsafe {
        let byte = c as u8;
        let mut ptr = s;
        loop {
            if *ptr as u8 == byte {
                return ptr as *mut c_char;
            }
            if *ptr == 0 {
                return core::ptr::null_mut();
            }
            ptr = ptr.add(1);
        }
    }
}

/// strcmp - compare strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    unsafe {
        let mut i = 0;
        loop {
            let c1 = *s1.add(i);
            let c2 = *s2.add(i);
            if c1 != c2 || c1 == 0 {
                return (c1 as c_int) - (c2 as c_int);
            }
            i += 1;
        }
    }
}

/// strncmp - compare strings up to n bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    unsafe {
        for i in 0..n {
            let c1 = *s1.add(i);
            let c2 = *s2.add(i);
            if c1 != c2 || c1 == 0 {
                return (c1 as c_int) - (c2 as c_int);
            }
        }
        0
    }
}

/// strncpy - copy string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(dest: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
    unsafe {
        let mut i = 0;
        while i < n && *src.add(i) != 0 {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
        while i < n {
            *dest.add(i) = 0;
            i += 1;
        }
        dest
    }
}

/// strtoul - convert string to unsigned long
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoul(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    unsafe {
        let mut ptr = nptr;
        let mut result: c_ulong = 0;

        // Skip whitespace (cast to u8 for byte comparisons)
        while *ptr as u8 == b' ' || *ptr as u8 == b'\t' {
            ptr = ptr.add(1);
        }

        // Determine base
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

        // Parse digits
        loop {
            // Cast to u8 for byte comparisons (c_char may be i8 on some platforms)
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

/// Global errno value (referenced directly by zenoh-pico C code compiled with picolibc)
#[unsafe(no_mangle)]
pub static mut errno: c_int = 0;

/// __errno - get errno pointer (for newlib compatibility)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __errno() -> *mut c_int {
    // Use raw pointer to avoid creating mutable reference to static
    &raw mut errno
}

// NOTE: __assert_func is NOT defined here for ESP32.
// The ESP32 runtime (esp-backtrace) provides its own __assert_func.
// Defining it here would cause duplicate symbol errors with LTO.

// ============================================================================
// snprintf/sprintf stubs
//
// Without esp-radio (WiFi), nobody provides these. zenoh-pico's debug output
// is disabled, but the symbols may still be referenced. These stubs write
// nothing and return 0.
// ============================================================================

/// snprintf stub - writes nothing, returns 0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(
    _buf: *mut c_char,
    _size: usize,
    _fmt: *const c_char,
    // C variadic args follow but we ignore them
) -> c_int {
    0
}

/// sprintf stub - writes nothing, returns 0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sprintf(
    _buf: *mut c_char,
    _fmt: *const c_char,
    // C variadic args follow but we ignore them
) -> c_int {
    0
}

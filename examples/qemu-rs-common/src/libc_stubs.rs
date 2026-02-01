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
#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

/// memcpy - copy memory
#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;
    for i in 0..n {
        *d.add(i) = *s.add(i);
    }
    dest
}

/// memmove - copy memory (handles overlapping regions)
#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
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

/// memset - fill memory
#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut c_void, c: c_int, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    for i in 0..n {
        *d.add(i) = c as u8;
    }
    dest
}

/// memcmp - compare memory
#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int {
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

/// memchr - find byte in memory
#[no_mangle]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let ptr = s as *const u8;
    let byte = c as u8;
    for i in 0..n {
        if *ptr.add(i) == byte {
            return ptr.add(i) as *mut c_void;
        }
    }
    core::ptr::null_mut()
}

/// strcmp - compare strings
#[no_mangle]
pub unsafe extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let c1 = *s1.add(i) as u8;
        let c2 = *s2.add(i) as u8;
        if c1 != c2 || c1 == 0 {
            return (c1 as c_int) - (c2 as c_int);
        }
        i += 1;
    }
}

/// strncmp - compare strings up to n bytes
#[no_mangle]
pub unsafe extern "C" fn strncmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    for i in 0..n {
        let c1 = *s1.add(i) as u8;
        let c2 = *s2.add(i) as u8;
        if c1 != c2 || c1 == 0 {
            return (c1 as c_int) - (c2 as c_int);
        }
    }
    0
}

/// strncpy - copy string
#[no_mangle]
pub unsafe extern "C" fn strncpy(dest: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
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

/// strtoul - convert string to unsigned long
#[no_mangle]
pub unsafe extern "C" fn strtoul(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    let mut ptr = nptr;
    let mut result: c_ulong = 0;

    // Skip whitespace
    while *ptr == b' ' as c_char || *ptr == b'\t' as c_char {
        ptr = ptr.add(1);
    }

    // Determine base
    let radix = if base == 0 {
        if *ptr == b'0' as c_char {
            ptr = ptr.add(1);
            if *ptr == b'x' as c_char || *ptr == b'X' as c_char {
                ptr = ptr.add(1);
                16
            } else {
                8
            }
        } else {
            10
        }
    } else if base == 16 && *ptr == b'0' as c_char {
        ptr = ptr.add(1);
        if *ptr == b'x' as c_char || *ptr == b'X' as c_char {
            ptr = ptr.add(1);
        }
        16
    } else {
        base as c_ulong
    };

    // Parse digits
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

/// Global errno value
static mut ERRNO: c_int = 0;

/// __errno - get errno pointer (for newlib compatibility)
#[no_mangle]
pub unsafe extern "C" fn __errno() -> *mut c_int {
    // Use raw pointer to avoid creating mutable reference to static
    &raw mut ERRNO
}

/// __assert_func - assertion failure handler
#[no_mangle]
pub unsafe extern "C" fn __assert_func(
    _file: *const c_char,
    _line: c_int,
    _func: *const c_char,
    _expr: *const c_char,
) -> ! {
    // In a real application, we might want to print something here
    // For now, just halt
    loop {
        cortex_m::asm::bkpt();
    }
}

// Note: snprintf is not implemented because it requires C variadic functions
// which need nightly Rust. zenoh-pico's debug output is disabled so this
// shouldn't be called. If it is needed, provide the function from C.

/// sprintf - stub (returns 0, writes nothing)
/// This is intentionally not implemented because zenoh-pico debug is disabled.
#[no_mangle]
pub unsafe extern "C" fn sprintf(_buf: *mut c_char, _fmt: *const c_char) -> c_int {
    0
}

/// snprintf - stub (returns 0, writes nothing)
/// This is intentionally not implemented because zenoh-pico debug is disabled.
#[no_mangle]
pub unsafe extern "C" fn snprintf(buf: *mut c_char, size: usize, _fmt: *const c_char) -> c_int {
    if !buf.is_null() && size > 0 {
        *buf = 0;
    }
    0
}

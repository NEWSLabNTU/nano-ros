//! Minimal libc stubs for bare-metal MPS2-AN385.
//!
//! These functions are required by C libraries (zenoh-pico, XRCE-DDS)
//! but not available in no_std. They provide minimal implementations
//! sufficient for embedded use.
//!
//! These are exported as `#[unsafe(no_mangle)]` symbols directly — they
//! are not part of the platform trait interface since they are C standard
//! library functions resolved at link time.

#![allow(clippy::missing_safety_doc)]
#![allow(clippy::unnecessary_cast)]

use core::ffi::{c_char, c_int, c_ulong, c_void};

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    unsafe {
        let d = dest as *mut u8;
        let s = src as *const u8;
        if (d as usize) < (s as usize) {
            for i in 0..n {
                *d.add(i) = *s.add(i);
            }
        } else {
            for i in (0..n).rev() {
                *d.add(i) = *s.add(i);
            }
        }
        dest
    }
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __assert_func(
    _file: *const c_char,
    _line: c_int,
    _func: *const c_char,
    _expr: *const c_char,
) -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sprintf(_buf: *mut c_char, _fmt: *const c_char) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(buf: *mut c_char, size: usize, _fmt: *const c_char) -> c_int {
    if !buf.is_null() && size > 0 {
        unsafe { *buf = 0 };
    }
    0
}

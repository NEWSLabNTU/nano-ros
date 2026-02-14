//! CDR serialization helpers for generated C message types.
//!
//! These functions are used by generated message serialization/deserialization code.
//! They handle CDR (Common Data Representation) encoding with little-endian byte order.

use core::ffi::c_char;

/// Write a boolean value to the buffer.
///
/// # Safety
/// - `ptr` must point to a valid mutable pointer to a buffer
/// - The buffer must have sufficient space
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_bool(
    ptr: *mut *mut u8,
    end: *const u8,
    value: bool,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() {
        return -1;
    }
    let p = *ptr;
    if p >= end as *mut u8 {
        return -1;
    }
    *p = if value { 1 } else { 0 };
    *ptr = p.add(1);
    0
}

/// Write a u8 value to the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_u8(
    ptr: *mut *mut u8,
    end: *const u8,
    value: u8,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() {
        return -1;
    }
    let p = *ptr;
    if p >= end as *mut u8 {
        return -1;
    }
    *p = value;
    *ptr = p.add(1);
    0
}

/// Write an i8 value to the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_i8(
    ptr: *mut *mut u8,
    end: *const u8,
    value: i8,
) -> i32 {
    nano_ros_cdr_write_u8(ptr, end, value as u8)
}

/// Align pointer to the specified alignment.
unsafe fn align_ptr(ptr: *mut *mut u8, end: *const u8, align: usize) -> i32 {
    let p = *ptr as usize;
    let aligned = (p + align - 1) & !(align - 1);
    let padding = aligned - p;
    if (*ptr).add(padding) > end as *mut u8 {
        return -1;
    }
    // Zero-fill padding bytes
    for i in 0..padding {
        *(*ptr).add(i) = 0;
    }
    *ptr = aligned as *mut u8;
    0
}

/// Write a u16 value to the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_u16(
    ptr: *mut *mut u8,
    end: *const u8,
    value: u16,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() {
        return -1;
    }
    if align_ptr(ptr, end, 2) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(2) > end as *mut u8 {
        return -1;
    }
    // Little-endian
    *p = (value & 0xFF) as u8;
    *p.add(1) = ((value >> 8) & 0xFF) as u8;
    *ptr = p.add(2);
    0
}

/// Write an i16 value to the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_i16(
    ptr: *mut *mut u8,
    end: *const u8,
    value: i16,
) -> i32 {
    nano_ros_cdr_write_u16(ptr, end, value as u16)
}

/// Write a u32 value to the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_u32(
    ptr: *mut *mut u8,
    end: *const u8,
    value: u32,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() {
        return -1;
    }
    if align_ptr(ptr, end, 4) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(4) > end as *mut u8 {
        return -1;
    }
    // Little-endian
    *p = (value & 0xFF) as u8;
    *p.add(1) = ((value >> 8) & 0xFF) as u8;
    *p.add(2) = ((value >> 16) & 0xFF) as u8;
    *p.add(3) = ((value >> 24) & 0xFF) as u8;
    *ptr = p.add(4);
    0
}

/// Write an i32 value to the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_i32(
    ptr: *mut *mut u8,
    end: *const u8,
    value: i32,
) -> i32 {
    nano_ros_cdr_write_u32(ptr, end, value as u32)
}

/// Write a u64 value to the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_u64(
    ptr: *mut *mut u8,
    end: *const u8,
    value: u64,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() {
        return -1;
    }
    if align_ptr(ptr, end, 8) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(8) > end as *mut u8 {
        return -1;
    }
    // Little-endian
    for i in 0..8 {
        *p.add(i) = ((value >> (i * 8)) & 0xFF) as u8;
    }
    *ptr = p.add(8);
    0
}

/// Write an i64 value to the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_i64(
    ptr: *mut *mut u8,
    end: *const u8,
    value: i64,
) -> i32 {
    nano_ros_cdr_write_u64(ptr, end, value as u64)
}

/// Write a f32 value to the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_f32(
    ptr: *mut *mut u8,
    end: *const u8,
    value: f32,
) -> i32 {
    nano_ros_cdr_write_u32(ptr, end, value.to_bits())
}

/// Write a f64 value to the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_f64(
    ptr: *mut *mut u8,
    end: *const u8,
    value: f64,
) -> i32 {
    nano_ros_cdr_write_u64(ptr, end, value.to_bits())
}

/// Write a string to the buffer (length-prefixed).
///
/// CDR strings are encoded as: u32 length (including null terminator) + bytes + null terminator
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_write_string(
    ptr: *mut *mut u8,
    end: *const u8,
    value: *const c_char,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }

    // Calculate string length
    let mut len: usize = 0;
    let mut s = value;
    while *s != 0 {
        len += 1;
        s = s.add(1);
    }

    // Write length (including null terminator)
    let total_len = (len + 1) as u32;
    if nano_ros_cdr_write_u32(ptr, end, total_len) < 0 {
        return -1;
    }

    // Check space for string + null
    let p = *ptr;
    if p.add(len + 1) > end as *mut u8 {
        return -1;
    }

    // Copy string bytes
    for i in 0..len {
        *p.add(i) = *value.add(i) as u8;
    }
    // Null terminator
    *p.add(len) = 0;

    *ptr = p.add(len + 1);
    0
}

// =============================================================================
// Read functions
// =============================================================================

/// Align read pointer to the specified alignment.
unsafe fn align_read_ptr(ptr: *mut *const u8, end: *const u8, align: usize) -> i32 {
    let p = *ptr as usize;
    let aligned = (p + align - 1) & !(align - 1);
    let new_ptr = aligned as *const u8;
    if new_ptr > end {
        return -1;
    }
    *ptr = new_ptr;
    0
}

/// Read a boolean value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_bool(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut bool,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }
    let p = *ptr;
    if p >= end {
        return -1;
    }
    *value = *p != 0;
    *ptr = p.add(1);
    0
}

/// Read a u8 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_u8(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut u8,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }
    let p = *ptr;
    if p >= end {
        return -1;
    }
    *value = *p;
    *ptr = p.add(1);
    0
}

/// Read an i8 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_i8(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut i8,
) -> i32 {
    nano_ros_cdr_read_u8(ptr, end, value as *mut u8)
}

/// Read a u16 value from the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_u16(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut u16,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }
    if align_read_ptr(ptr, end, 2) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(2) > end {
        return -1;
    }
    // Little-endian
    *value = (*p as u16) | ((*p.add(1) as u16) << 8);
    *ptr = p.add(2);
    0
}

/// Read an i16 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_i16(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut i16,
) -> i32 {
    nano_ros_cdr_read_u16(ptr, end, value as *mut u16)
}

/// Read a u32 value from the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_u32(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut u32,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }
    if align_read_ptr(ptr, end, 4) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(4) > end {
        return -1;
    }
    // Little-endian
    *value = (*p as u32)
        | ((*p.add(1) as u32) << 8)
        | ((*p.add(2) as u32) << 16)
        | ((*p.add(3) as u32) << 24);
    *ptr = p.add(4);
    0
}

/// Read an i32 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_i32(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut i32,
) -> i32 {
    nano_ros_cdr_read_u32(ptr, end, value as *mut u32)
}

/// Read a u64 value from the buffer (with alignment).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_u64(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut u64,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() {
        return -1;
    }
    if align_read_ptr(ptr, end, 8) < 0 {
        return -1;
    }
    let p = *ptr;
    if p.add(8) > end {
        return -1;
    }
    // Little-endian
    let mut v: u64 = 0;
    for i in 0..8 {
        v |= (*p.add(i) as u64) << (i * 8);
    }
    *value = v;
    *ptr = p.add(8);
    0
}

/// Read an i64 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_i64(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut i64,
) -> i32 {
    nano_ros_cdr_read_u64(ptr, end, value as *mut u64)
}

/// Read a f32 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_f32(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut f32,
) -> i32 {
    let mut bits: u32 = 0;
    let result = nano_ros_cdr_read_u32(ptr, end, &mut bits);
    if result < 0 {
        return result;
    }
    unsafe {
        *value = f32::from_bits(bits);
    }
    0
}

/// Read a f64 value from the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_f64(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut f64,
) -> i32 {
    let mut bits: u64 = 0;
    let result = nano_ros_cdr_read_u64(ptr, end, &mut bits);
    if result < 0 {
        return result;
    }
    unsafe {
        *value = f64::from_bits(bits);
    }
    0
}

/// Read a string from the buffer into a fixed-size buffer.
///
/// CDR strings are encoded as: u32 length (including null terminator) + bytes + null terminator
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_cdr_read_string(
    ptr: *mut *const u8,
    end: *const u8,
    value: *mut c_char,
    max_len: usize,
) -> i32 {
    if ptr.is_null() || (*ptr).is_null() || value.is_null() || max_len == 0 {
        return -1;
    }

    // Read length
    let mut len: u32 = 0;
    if nano_ros_cdr_read_u32(ptr, end, &mut len) < 0 {
        return -1;
    }

    let p = *ptr;
    let str_len = len as usize;

    // Check bounds
    if p.add(str_len) > end {
        return -1;
    }

    // Check destination buffer size (need space for null terminator)
    if str_len > max_len {
        return -1;
    }

    // Copy string (including null terminator if present)
    let copy_len = if str_len > 0 { str_len - 1 } else { 0 }; // Exclude null from copy count
    for i in 0..copy_len {
        *value.add(i) = *p.add(i) as c_char;
    }
    // Ensure null termination
    *value.add(copy_len) = 0;

    *ptr = p.add(str_len);
    0
}

#[cfg(kani)]
mod verification {
    use super::*;

    // =========================================================================
    // Null safety — write functions
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_u8_null_safety() {
        let end: *const u8 = core::ptr::null();
        // NULL ptr → -1
        assert_eq!(
            unsafe { nano_ros_cdr_write_u8(core::ptr::null_mut(), end, 0) },
            -1
        );
        // NULL *ptr → -1
        let mut null_inner: *mut u8 = core::ptr::null_mut();
        let buf = [0u8; 4];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_write_u8(&mut null_inner, end, 0) },
            -1
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_u32_null_safety() {
        let end: *const u8 = core::ptr::null();
        assert_eq!(
            unsafe { nano_ros_cdr_write_u32(core::ptr::null_mut(), end, 0) },
            -1
        );
        let mut null_inner: *mut u8 = core::ptr::null_mut();
        let buf = [0u8; 8];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_write_u32(&mut null_inner, end, 0) },
            -1
        );
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn cdr_write_u64_null_safety() {
        let end: *const u8 = core::ptr::null();
        assert_eq!(
            unsafe { nano_ros_cdr_write_u64(core::ptr::null_mut(), end, 0) },
            -1
        );
        let mut null_inner: *mut u8 = core::ptr::null_mut();
        let buf = [0u8; 16];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_write_u64(&mut null_inner, end, 0) },
            -1
        );
    }

    // =========================================================================
    // Null safety — read functions
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_read_u8_null_safety() {
        let end: *const u8 = core::ptr::null();
        let mut val: u8 = 0;
        // NULL ptr → -1
        assert_eq!(
            unsafe { nano_ros_cdr_read_u8(core::ptr::null_mut(), end, &mut val) },
            -1
        );
        // NULL *ptr → -1
        let mut null_inner: *const u8 = core::ptr::null();
        let buf = [0u8; 4];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_read_u8(&mut null_inner, end, &mut val) },
            -1
        );
        // NULL value → -1
        let mut rptr: *const u8 = buf.as_ptr();
        assert_eq!(
            unsafe { nano_ros_cdr_read_u8(&mut rptr, end, core::ptr::null_mut()) },
            -1
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_read_u32_null_safety() {
        let end: *const u8 = core::ptr::null();
        let mut val: u32 = 0;
        assert_eq!(
            unsafe { nano_ros_cdr_read_u32(core::ptr::null_mut(), end, &mut val) },
            -1
        );
        let mut null_inner: *const u8 = core::ptr::null();
        let buf = [0u8; 8];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_read_u32(&mut null_inner, end, &mut val) },
            -1
        );
        let mut rptr: *const u8 = buf.as_ptr();
        assert_eq!(
            unsafe { nano_ros_cdr_read_u32(&mut rptr, end, core::ptr::null_mut()) },
            -1
        );
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn cdr_read_u64_null_safety() {
        let end: *const u8 = core::ptr::null();
        let mut val: u64 = 0;
        assert_eq!(
            unsafe { nano_ros_cdr_read_u64(core::ptr::null_mut(), end, &mut val) },
            -1
        );
        let mut null_inner: *const u8 = core::ptr::null();
        let buf = [0u8; 16];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_read_u64(&mut null_inner, end, &mut val) },
            -1
        );
        let mut rptr: *const u8 = buf.as_ptr();
        assert_eq!(
            unsafe { nano_ros_cdr_read_u64(&mut rptr, end, core::ptr::null_mut()) },
            -1
        );
    }

    // =========================================================================
    // Buffer bounds — write functions (insufficient space → -1, no OOB write)
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_u8_bounds() {
        // Zero-length buffer → -1
        let mut buf = [0u8; 1];
        let end = buf.as_ptr(); // end == start, zero capacity
        let mut wptr = buf.as_mut_ptr();
        assert_eq!(unsafe { nano_ros_cdr_write_u8(&mut wptr, end, 42) }, -1);
    }

    // NOTE: Buffer bounds and alignment harnesses for multi-byte types (u32, u64)
    // are not included because:
    //
    // 1. align_ptr() uses pointer-to-integer-to-pointer round-trips for alignment
    //    arithmetic (`*ptr as usize` → align → `aligned as *mut u8`), which CBMC's
    //    pointer model cannot track across allocation boundaries.
    //
    // 2. Bounds-checking code uses `ptr.add(N) > end` where N may exceed the
    //    allocation, which Kani flags as a pointer offset violation even though
    //    the result is only used in a comparison.
    //
    // These properties are verified by the existing #[test] unit tests
    // (test_alignment, test_write_read_u32, etc.) and by Miri (`just test-miri`
    // on nros-serdes which uses the same CDR logic in safe Rust).
    //
    // The round-trip harnesses below (u32, u64) succeed because they start at
    // offset 0 where no alignment padding is needed.

    // =========================================================================
    // Round-trip correctness — write then read preserves value
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_roundtrip_u8() {
        let mut buf = [0u8; 4];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        let val: u8 = kani::any();

        let mut wptr = buf.as_mut_ptr();
        let wret = unsafe { nano_ros_cdr_write_u8(&mut wptr, end, val) };
        assert_eq!(wret, 0);

        let mut rptr: *const u8 = buf.as_ptr();
        let mut out: u8 = 0;
        let rret = unsafe { nano_ros_cdr_read_u8(&mut rptr, end, &mut out) };
        assert_eq!(rret, 0);
        assert_eq!(out, val);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_roundtrip_bool() {
        let mut buf = [0u8; 4];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        let val: bool = kani::any();

        let mut wptr = buf.as_mut_ptr();
        let wret = unsafe { nano_ros_cdr_write_bool(&mut wptr, end, val) };
        assert_eq!(wret, 0);

        let mut rptr: *const u8 = buf.as_ptr();
        let mut out: bool = false;
        let rret = unsafe { nano_ros_cdr_read_bool(&mut rptr, end, &mut out) };
        assert_eq!(rret, 0);
        assert_eq!(out, val);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_roundtrip_u32() {
        let mut buf = [0u8; 16];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        let val: u32 = kani::any();

        let mut wptr = buf.as_mut_ptr();
        let wret = unsafe { nano_ros_cdr_write_u32(&mut wptr, end, val) };
        assert_eq!(wret, 0);

        let mut rptr: *const u8 = buf.as_ptr();
        let mut out: u32 = 0;
        let rret = unsafe { nano_ros_cdr_read_u32(&mut rptr, end, &mut out) };
        assert_eq!(rret, 0);
        assert_eq!(out, val);
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn cdr_roundtrip_u64() {
        let mut buf = [0u8; 16];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        let val: u64 = kani::any();

        let mut wptr = buf.as_mut_ptr();
        let wret = unsafe { nano_ros_cdr_write_u64(&mut wptr, end, val) };
        assert_eq!(wret, 0);

        let mut rptr: *const u8 = buf.as_ptr();
        let mut out: u64 = 0;
        let rret = unsafe { nano_ros_cdr_read_u64(&mut rptr, end, &mut out) };
        assert_eq!(rret, 0);
        assert_eq!(out, val);
    }

    // =========================================================================
    // String — null safety
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_string_null_safety() {
        let end: *const u8 = core::ptr::null();
        // NULL ptr → -1
        assert_eq!(
            unsafe { nano_ros_cdr_write_string(core::ptr::null_mut(), end, core::ptr::null()) },
            -1
        );
        // NULL *ptr → -1
        let mut null_inner: *mut u8 = core::ptr::null_mut();
        let buf = [0u8; 64];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe {
                nano_ros_cdr_write_string(&mut null_inner, end, b"hi\0".as_ptr() as *const c_char)
            },
            -1
        );
        // NULL value string → -1
        let mut wptr = buf.as_ptr() as *mut u8;
        assert_eq!(
            unsafe { nano_ros_cdr_write_string(&mut wptr, end, core::ptr::null()) },
            -1
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_read_string_null_safety() {
        let end: *const u8 = core::ptr::null();
        let mut val = [0i8; 16];
        // NULL ptr → -1
        assert_eq!(
            unsafe {
                nano_ros_cdr_read_string(core::ptr::null_mut(), end, val.as_mut_ptr(), val.len())
            },
            -1
        );
        // NULL *ptr → -1
        let mut null_inner: *const u8 = core::ptr::null();
        let buf = [0u8; 64];
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        assert_eq!(
            unsafe { nano_ros_cdr_read_string(&mut null_inner, end, val.as_mut_ptr(), val.len()) },
            -1
        );
        // NULL value buffer → -1
        let mut rptr: *const u8 = buf.as_ptr();
        assert_eq!(
            unsafe { nano_ros_cdr_read_string(&mut rptr, end, core::ptr::null_mut(), 16) },
            -1
        );
        // max_len == 0 → -1
        let mut rptr: *const u8 = buf.as_ptr();
        assert_eq!(
            unsafe { nano_ros_cdr_read_string(&mut rptr, end, val.as_mut_ptr(), 0) },
            -1
        );
    }

    // =========================================================================
    // String — buffer bounds
    // =========================================================================

    // NOTE: cdr_write_string_bounds is not included due to the same pointer
    // offset limitation described above (the bounds-check `p.add(len+1) > end`
    // creates an out-of-bounds intermediate pointer).

    #[kani::proof]
    #[kani::unwind(10)]
    fn cdr_read_string_bounds() {
        // Write a valid string then try to read with max_len too small
        let mut buf = [0u8; 64];
        let end = unsafe { buf.as_ptr().add(buf.len()) };

        // Write "Hello\0"
        let mut wptr = buf.as_mut_ptr();
        let s = b"Hello\0";
        let wret =
            unsafe { nano_ros_cdr_write_string(&mut wptr, end, s.as_ptr() as *const c_char) };
        assert_eq!(wret, 0);

        // Read with max_len = 2 (too small for "Hello" + null = 6 bytes; CDR len includes null = 6)
        let mut rptr: *const u8 = buf.as_ptr();
        let mut val = [0i8; 2];
        let rret = unsafe { nano_ros_cdr_read_string(&mut rptr, end, val.as_mut_ptr(), val.len()) };
        assert_eq!(rret, -1);
    }

    // =========================================================================
    // String — round-trip
    // =========================================================================

    #[kani::proof]
    #[kani::unwind(10)]
    fn cdr_roundtrip_string() {
        let mut buf = [0u8; 64];
        let end = unsafe { buf.as_ptr().add(buf.len()) };

        // Write "Hi\0"
        let s = b"Hi\0";
        let mut wptr = buf.as_mut_ptr();
        let wret =
            unsafe { nano_ros_cdr_write_string(&mut wptr, end, s.as_ptr() as *const c_char) };
        assert_eq!(wret, 0);

        // Read back
        let mut rptr: *const u8 = buf.as_ptr();
        let mut val = [0i8; 32];
        let rret = unsafe { nano_ros_cdr_read_string(&mut rptr, end, val.as_mut_ptr(), val.len()) };
        assert_eq!(rret, 0);

        // Verify content preserved
        assert_eq!(val[0], b'H' as i8);
        assert_eq!(val[1], b'i' as i8);
        // Verify null-terminated
        assert_eq!(val[2], 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read_u32() {
        let mut buffer = [0u8; 16];
        let mut ptr = buffer.as_mut_ptr();
        let end = unsafe { buffer.as_ptr().add(buffer.len()) };

        // Write
        unsafe {
            assert_eq!(nano_ros_cdr_write_u32(&mut ptr, end, 0x12345678), 0);
        }

        // Read
        let mut read_ptr = buffer.as_ptr();
        let mut value: u32 = 0;
        unsafe {
            assert_eq!(nano_ros_cdr_read_u32(&mut read_ptr, end, &mut value), 0);
        }
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_write_read_i32() {
        let mut buffer = [0u8; 16];
        let mut ptr = buffer.as_mut_ptr();
        let end = unsafe { buffer.as_ptr().add(buffer.len()) };

        // Write negative value
        unsafe {
            assert_eq!(nano_ros_cdr_write_i32(&mut ptr, end, -12345), 0);
        }

        // Read
        let mut read_ptr = buffer.as_ptr();
        let mut value: i32 = 0;
        unsafe {
            assert_eq!(nano_ros_cdr_read_i32(&mut read_ptr, end, &mut value), 0);
        }
        assert_eq!(value, -12345);
    }

    #[test]
    fn test_write_read_f64() {
        let mut buffer = [0u8; 16];
        let mut ptr = buffer.as_mut_ptr();
        let end = unsafe { buffer.as_ptr().add(buffer.len()) };

        // Write
        unsafe {
            assert_eq!(nano_ros_cdr_write_f64(&mut ptr, end, 3.14159265358979), 0);
        }

        // Read
        let mut read_ptr = buffer.as_ptr();
        let mut value: f64 = 0.0;
        unsafe {
            assert_eq!(nano_ros_cdr_read_f64(&mut read_ptr, end, &mut value), 0);
        }
        assert!((value - 3.14159265358979).abs() < 1e-15);
    }

    #[test]
    fn test_write_read_string() {
        let mut buffer = [0u8; 64];
        let mut ptr = buffer.as_mut_ptr();
        let end = unsafe { buffer.as_ptr().add(buffer.len()) };

        // Write
        let test_str = b"Hello, World!\0";
        unsafe {
            assert_eq!(
                nano_ros_cdr_write_string(&mut ptr, end, test_str.as_ptr() as *const c_char),
                0
            );
        }

        // Read
        let mut read_ptr = buffer.as_ptr();
        let mut value = [0i8; 32];
        unsafe {
            assert_eq!(
                nano_ros_cdr_read_string(&mut read_ptr, end, value.as_mut_ptr(), value.len()),
                0
            );
        }

        // Convert to string and compare
        let result = unsafe { std::ffi::CStr::from_ptr(value.as_ptr()).to_str().unwrap() };
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_alignment() {
        let mut buffer = [0u8; 32];
        let mut ptr = buffer.as_mut_ptr();
        let end = unsafe { buffer.as_ptr().add(buffer.len()) };

        // Write u8, then u32 - should align to 4 bytes
        unsafe {
            assert_eq!(nano_ros_cdr_write_u8(&mut ptr, end, 0xAA), 0);
            assert_eq!(nano_ros_cdr_write_u32(&mut ptr, end, 0x12345678), 0);
        }

        // Check alignment: u8 at offset 0, u32 at offset 4
        assert_eq!(buffer[0], 0xAA);
        assert_eq!(buffer[1], 0); // padding
        assert_eq!(buffer[2], 0); // padding
        assert_eq!(buffer[3], 0); // padding
        assert_eq!(buffer[4], 0x78); // u32 little-endian
        assert_eq!(buffer[5], 0x56);
        assert_eq!(buffer[6], 0x34);
        assert_eq!(buffer[7], 0x12);
    }
}

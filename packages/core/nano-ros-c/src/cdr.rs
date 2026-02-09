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

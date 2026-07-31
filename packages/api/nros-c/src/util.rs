//! Internal helpers shared by FFI entry points.

use core::ffi::c_char;

/// Copy a null-terminated C string into a fixed-size byte buffer, leaving
/// space for a trailing NUL byte. Returns the number of bytes copied,
/// excluding the trailing NUL.
///
/// If `src` is null, the destination is left untouched and `0` is
/// returned. Callers that require a non-empty source string should check
/// the return value and reject zero. The trailing NUL is always written
/// when `src` is non-null.
///
/// # Safety
/// `src` must be either null OR a valid pointer to a NUL-terminated C
/// string. `dst` must be a valid mutable reference for `N` bytes.
#[inline]
pub(crate) unsafe fn copy_cstr_into<const N: usize>(
    src: *const c_char,
    dst: &mut [u8; N],
) -> usize {
    if src.is_null() {
        return 0;
    }
    let src = src as *const u8;
    let cap = N - 1;
    let mut len = 0usize;
    while len < cap {
        let c = unsafe { *src.add(len) };
        if c == 0 {
            break;
        }
        dst[len] = c;
        len += 1;
    }
    dst[len] = 0;
    len
}

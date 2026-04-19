//! Xorshift32 PRNG for bare-metal platforms.
//!
//! Suitable for TCP sequence numbers and zenoh session IDs. Not a CSPRNG.
//!
//! Each platform crate's `random_u8()` / `random_u32()` / etc.
//! inherent methods delegate to these free functions.

static mut RNG_STATE: u32 = 0x12345678;

/// Seed the PRNG.
pub fn seed(value: u32) {
    unsafe {
        RNG_STATE = if value == 0 { 0x12345678 } else { value };
    }
}

/// Generate a random u32 using xorshift.
fn next_u32() -> u32 {
    unsafe {
        let mut x = RNG_STATE;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG_STATE = x;
        x
    }
}

pub fn random_u8() -> u8 {
    (next_u32() & 0xFF) as u8
}

pub fn random_u16() -> u16 {
    (next_u32() & 0xFFFF) as u16
}

pub fn random_u32() -> u32 {
    next_u32()
}

pub fn random_u64() -> u64 {
    let high = next_u32() as u64;
    let low = next_u32() as u64;
    (high << 32) | low
}

pub fn random_fill(buf: *mut core::ffi::c_void, len: usize) {
    if buf.is_null() {
        return;
    }
    let ptr = buf as *mut u8;
    let mut remaining = len;
    let mut offset = 0;

    while remaining >= 4 {
        let r = next_u32();
        let bytes = r.to_ne_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), 4);
        }
        offset += 4;
        remaining -= 4;
    }

    if remaining > 0 {
        let r = next_u32();
        let bytes = r.to_ne_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), remaining);
        }
    }
}

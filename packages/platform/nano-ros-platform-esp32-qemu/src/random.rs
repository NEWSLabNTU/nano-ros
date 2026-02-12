//! LFSR xorshift PRNG for zenoh-pico
//!
//! Provides `z_random_u8/u16/u32/u64` and `z_random_fill` implementations.
//! Seeded from ESP32-C3 hardware RNG in `run_node()`.

static mut RNG_STATE: u32 = 0x12345678;

/// Seed the PRNG
pub fn seed(value: u32) {
    unsafe {
        RNG_STATE = if value == 0 { 0x12345678 } else { value };
    }
}

/// Generate a random u32 using xorshift
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

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u8() -> u8 {
    (next_u32() & 0xFF) as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u16() -> u16 {
    (next_u32() & 0xFFFF) as u16
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u32() -> u32 {
    next_u32()
}

#[unsafe(no_mangle)]
pub extern "C" fn z_random_u64() -> u64 {
    let high = next_u32() as u64;
    let low = next_u32() as u64;
    (high << 32) | low
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_random_fill(buf: *mut core::ffi::c_void, len: usize) {
    if buf.is_null() {
        return;
    }
    let ptr = buf as *mut u8;
    let mut remaining = len;
    let mut offset = 0;

    // Fill in 4-byte chunks
    while remaining >= 4 {
        let r = next_u32();
        let bytes = r.to_ne_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), 4);
        }
        offset += 4;
        remaining -= 4;
    }

    // Fill remaining bytes
    if remaining > 0 {
        let r = next_u32();
        let bytes = r.to_ne_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), remaining);
        }
    }
}

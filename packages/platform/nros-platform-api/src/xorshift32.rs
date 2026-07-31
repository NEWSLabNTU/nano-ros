//! Xorshift32 PRNG helpers shared by RTOS platform crates that lack a
//! hardware RNG.
//!
//! The state is owned by the caller (typically a `static mut` inside the
//! platform crate), so multiple platforms can share these helpers without
//! the API crate carrying mutable state.
//!
//! Quality is sufficient for zenoh session-ID seeding — not for
//! cryptography. Pair with hardware entropy at boot (clock, MAC address,
//! ADC noise) via `seed()`.

/// Default seed used when entropy is unavailable. Xorshift cannot escape
/// an all-zero state, so callers that pass `0` to [`seed`] fall back to
/// this value.
pub const DEFAULT_SEED: u32 = 0x12345678;

/// One xorshift32 step. Pure function — caller manages the state cell.
#[inline]
pub const fn step(state: u32) -> u32 {
    let mut x = state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

/// Advance `state` by one step and return the new value.
///
/// # Safety
/// `state` must be a valid, exclusively-owned `*mut u32`. Single-threaded
/// RTOS init code typically guarantees this; SMP callers must wrap in a
/// critical section.
#[inline]
pub unsafe fn next(state: *mut u32) -> u32 {
    let next = step(unsafe { *state });
    unsafe { *state = next };
    next
}

/// Replace `state` with `value`, falling back to [`DEFAULT_SEED`] if
/// `value == 0`.
///
/// # Safety
/// `state` must be a valid, exclusively-owned `*mut u32`.
#[inline]
pub unsafe fn seed(state: *mut u32, value: u32) {
    unsafe { *state = if value == 0 { DEFAULT_SEED } else { value } };
}

/// Fill `buf[..len]` with bytes derived from xorshift32 output. No-op if
/// `buf` is null.
///
/// # Safety
/// `buf` must be valid for `len` writes. `state` must be exclusively
/// owned.
pub unsafe fn random_fill(state: *mut u32, buf: *mut u8, len: usize) {
    if buf.is_null() {
        return;
    }
    let mut offset = 0;
    let mut remaining = len;
    while remaining >= 4 {
        let bytes = unsafe { next(state) }.to_ne_bytes();
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.add(offset), 4) };
        offset += 4;
        remaining -= 4;
    }
    if remaining > 0 {
        let bytes = unsafe { next(state) }.to_ne_bytes();
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.add(offset), remaining) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_is_deterministic() {
        assert_eq!(step(DEFAULT_SEED), step(DEFAULT_SEED));
    }

    #[test]
    fn step_changes_state() {
        assert_ne!(step(DEFAULT_SEED), DEFAULT_SEED);
    }

    #[test]
    fn seed_zero_falls_back_to_default() {
        let mut s: u32 = 1;
        unsafe { seed(&raw mut s, 0) };
        assert_eq!(s, DEFAULT_SEED);
    }

    #[test]
    fn seed_nonzero_takes_value() {
        let mut s: u32 = 1;
        unsafe { seed(&raw mut s, 0xDEADBEEF) };
        assert_eq!(s, 0xDEADBEEF);
    }

    #[test]
    fn next_advances_state() {
        let mut s: u32 = DEFAULT_SEED;
        let v = unsafe { next(&raw mut s) };
        assert_eq!(v, step(DEFAULT_SEED));
        assert_eq!(s, v);
    }

    #[test]
    fn random_fill_null_buf_is_noop() {
        let mut s: u32 = DEFAULT_SEED;
        unsafe { random_fill(&raw mut s, core::ptr::null_mut(), 16) };
        assert_eq!(s, DEFAULT_SEED);
    }

    #[test]
    fn random_fill_writes_requested_length() {
        let mut s: u32 = DEFAULT_SEED;
        let mut buf = [0u8; 13];
        unsafe { random_fill(&raw mut s, buf.as_mut_ptr(), buf.len()) };
        assert!(buf.iter().any(|&b| b != 0));
    }
}

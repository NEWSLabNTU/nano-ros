//! `PlatformRandom` impl — best-effort xorshift32 seeded from the
//! FreeRTOS tick.
//!
//! # Quality
//!
//! The Orin SPE has **no hardware RNG** wired into the SPE address
//! space — the SoC's HWRNG sits behind the SE/CRNG block on the CCPLEX
//! side, unreachable from the AON cluster without IVC RPC. xorshift32
//! seeded from the tick is fine for zenoh-pico's needs (sequence-
//! number jitter, scout retransmit timing) and **categorically
//! inappropriate for cryptographic use**.
//!
//! If a future bring-up needs cryptographic randomness on the SPE,
//! the right answer is an IVC RPC to a CCPLEX-side getrandom service —
//! not a stronger algorithm here.
//!
//! The seed is reseeded lazily on first use from `clock_us()` so two
//! processes that boot at slightly different points pick different
//! sequences. The state is a single `AtomicU32` to keep this trait
//! impl `Send`-able across FreeRTOS tasks without a mutex.

use crate::OrinSpe;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};
use nros_platform_api::{PlatformClock, PlatformRandom};

static STATE: AtomicU32 = AtomicU32::new(0);

#[inline]
fn next_u32() -> u32 {
    let mut s = STATE.load(Ordering::Relaxed);
    if s == 0 {
        // Lazy seed. `clock_us` is monotonic since boot; mixing in the
        // low 32 bits gives a different starting point per cold start.
        let seed = OrinSpe::clock_us() as u32;
        // Avoid the all-zero state that xorshift32 cannot escape from.
        s = if seed == 0 { 0xdeadbeef } else { seed };
    }
    // Marsaglia's xorshift32.
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    STATE.store(s, Ordering::Relaxed);
    s
}

impl PlatformRandom for OrinSpe {
    fn random_u8() -> u8 {
        next_u32() as u8
    }

    fn random_u16() -> u16 {
        next_u32() as u16
    }

    fn random_u32() -> u32 {
        next_u32()
    }

    fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    fn random_fill(buf: *mut c_void, len: usize) {
        if buf.is_null() || len == 0 {
            return;
        }
        let mut p = buf as *mut u8;
        let end = unsafe { p.add(len) };
        while p < end {
            let chunk = next_u32().to_le_bytes();
            let remaining = unsafe { end.offset_from(p) } as usize;
            let n = remaining.min(4);
            for (i, byte) in chunk.iter().enumerate().take(n) {
                unsafe { p.add(i).write(*byte) };
            }
            p = unsafe { p.add(n) };
        }
    }
}

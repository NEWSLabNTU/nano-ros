//! Phase 141.B.2 — wake-latency probe.
//!
//! Two probe points on the executor's wake path:
//!
//! - [`on_wake`]: called from `nros_rmw_runtime_wake_cb` (std +
//!   alloc-mode variants). Captures `T0` — the cycle count at
//!   "transport notified executor".
//! - [`on_dispatch`]: called from the arena dispatch site when a
//!   subscription bit fires. Reads `T1` — "subscription callback
//!   ran" — and stores `(T1 - T0)` in the lock-free ring at
//!   [`SAMPLES`].
//!
//! Storage is `[AtomicU64; PROBE_SAMPLE_CAP]`; `WRITE_IDX`
//! wraps. The 141.C host harness reads the ring + the write
//! index over UART, parses, computes P99.
//!
//! Time source is caller-supplied via [`set_cycle_reader`].
//! On Cortex-M3 MPS2-AN385 callers point this at
//! `nros_platform_mps2_an385::timing::clock_cycles` (Phase
//! 141.B.1); other platforms can wire their own monotonic
//! cycle / nanosecond reader. Probe is allocation-free —
//! suitable for ISR call sites once `nros_rmw_runtime_wake_cb`'s
//! ISR-safe variant is wired (Phase 124.B.7).
//!
//! Gated behind `feature = "wake-latency-probe"` so production
//! builds carry zero overhead and the call sites become
//! `#[cfg]`-elided no-ops.

#![cfg(feature = "wake-latency-probe")]

// Cortex-M3 (thumbv7m) has no native AtomicU64 — use the
// portable-atomic crate's polyfill (CAS-based on 32-bit
// platforms). `nros-node`'s `alloc` feature already pulls
// `portable-atomic` in; reuse it here.
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use portable_atomic::AtomicU64;

/// Number of (T1 - T0) samples buffered before the ring wraps.
/// 256 samples * 8 bytes = 2 KB BSS — fits easily in the
/// Cortex-M3 MPS2-AN385's 4 MB SRAM. Bump if the 141.D
/// scenarios need longer runs without drain.
pub const PROBE_SAMPLE_CAP: usize = 256;

/// `(T1 - T0)` deltas in caller's cycle / ns units. Caller
/// converts to ns via the same source the cycle reader uses
/// (e.g. `nros_platform_mps2_an385::timing::cycles_to_ns`).
static SAMPLES: [AtomicU64; PROBE_SAMPLE_CAP] = {
    const ZERO: AtomicU64 = AtomicU64::new(0);
    [ZERO; PROBE_SAMPLE_CAP]
};

/// Monotonic write counter (not modulo). Index into `SAMPLES`
/// is `WRITE_IDX % PROBE_SAMPLE_CAP`. Drained value lets the
/// host harness distinguish "256 samples since boot" from
/// "ring wrapped 10× since boot".
static WRITE_IDX: AtomicU32 = AtomicU32::new(0);

/// Most-recent `T0` (wake-cb entry cycle count). `on_dispatch`
/// reads + clears via swap so a wake that fires but never
/// dispatches doesn't pollute the next pairing. `0` is the
/// "no pending wake" sentinel.
static LAST_WAKE_TICKS: AtomicU64 = AtomicU64::new(0);

/// Caller-supplied monotonic cycle reader. Wired up at app
/// startup. Signature is `extern "C" fn() -> u64` so a C ABI
/// callsite (e.g. nros-c / nros-cpp's app entry) can install
/// the same reader.
pub type CycleReader = unsafe extern "C" fn() -> u64;

/// `AtomicPtr<()>` holding the installed `CycleReader` fn
/// pointer. Plain `Option<CycleReader>` can't be atomic
/// because function pointers aren't `AtomicPtr<T>`'s `T`;
/// store as `*mut ()` and `transmute` on read.
static CYCLE_READER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the cycle reader. Call once at app startup before
/// any wake fires. Passing `None` disables the probe (probe
/// hooks become no-ops on the next call).
pub fn set_cycle_reader(reader: Option<CycleReader>) {
    let ptr = match reader {
        Some(f) => f as *mut (),
        None => core::ptr::null_mut(),
    };
    CYCLE_READER.store(ptr, Ordering::Release);
}

#[inline]
fn read_cycles() -> Option<u64> {
    let ptr = CYCLE_READER.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: pointer was installed via `set_cycle_reader`
        // from a real `CycleReader` fn pointer; same ABI on
        // load. Producer-side `Release` pairs with `Acquire`.
        let f: CycleReader = unsafe { core::mem::transmute(ptr) };
        Some(unsafe { f() })
    }
}

/// Called from `nros_rmw_runtime_wake_cb` (both std + alloc
/// variants). Captures the entry cycle count into
/// `LAST_WAKE_TICKS` so the matching `on_dispatch` can compute
/// `T1 - T0`.
///
/// O(1), allocation-free, lock-free. Safe to call from any
/// context the wake-cb itself is safe in (notably NOT ISR-safe
/// today — wake-cb's ISR contract is the limiting factor, not
/// the probe).
#[inline]
pub fn on_wake() {
    if let Some(t0) = read_cycles() {
        // SeqCst on store + acquire on the matching swap so
        // the dispatch site never reads a torn / stale T0.
        // A wake that arrives during a dispatch overwrites the
        // previous T0 — that's intentional; the previous wake's
        // dispatch already swapped it to 0.
        LAST_WAKE_TICKS.store(t0, Ordering::SeqCst);
    }
}

/// Called at the top of arena dispatch when a subscription bit
/// fires (`packages/core/nros-node/src/executor/spin.rs`
/// dispatch loop). Computes `T1 - T0` and pushes it into the
/// ring buffer. No-op if no pending wake (paired `on_wake`
/// already drained, or no wake fired since the last dispatch).
///
/// O(1), allocation-free, lock-free.
#[inline]
pub fn on_dispatch() {
    let Some(t1) = read_cycles() else { return };
    let t0 = LAST_WAKE_TICKS.swap(0, Ordering::AcqRel);
    if t0 == 0 {
        return;
    }
    // wrapping_sub handles the 32-bit DWT CYCCNT wrap when the
    // reader is `clock_cycles`. For ns / u64-monotonic readers
    // the wrap is irrelevant (u64 never wraps on practical
    // runs). Either way, the resulting delta is unsigned by
    // contract.
    let delta = t1.wrapping_sub(t0);
    let idx = WRITE_IDX.fetch_add(1, Ordering::Relaxed) as usize % PROBE_SAMPLE_CAP;
    SAMPLES[idx].store(delta, Ordering::Relaxed);
}

/// Drain the ring into a caller-supplied buffer + return the
/// number of samples and the monotonic write counter (so the
/// caller can detect ring wraps). Phase 141.C host harness
/// drives this over UART.
///
/// Returns `(written, total_writes_since_boot)`. `written =
/// min(out.len(), min(total, PROBE_SAMPLE_CAP))`.
pub fn drain(out: &mut [u64]) -> (usize, u32) {
    let total = WRITE_IDX.load(Ordering::Acquire);
    let available = (total as usize).min(PROBE_SAMPLE_CAP);
    let n = out.len().min(available);
    for (i, slot) in out.iter_mut().take(n).enumerate() {
        *slot = SAMPLES[i].load(Ordering::Relaxed);
    }
    (n, total)
}

/// Reset the probe — clear samples + write index + pending
/// wake. Useful for the 141.D scenarios that want a clean
/// histogram per scenario.
pub fn reset() {
    for slot in &SAMPLES {
        slot.store(0, Ordering::Relaxed);
    }
    WRITE_IDX.store(0, Ordering::Relaxed);
    LAST_WAKE_TICKS.store(0, Ordering::Relaxed);
}

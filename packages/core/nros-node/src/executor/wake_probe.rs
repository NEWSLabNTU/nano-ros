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

// ============================================================================
// Histogram (Phase 141.C.1)
// ============================================================================

/// Number of log-distributed buckets in the wake-latency histogram.
///
/// Phase 141.C.1 spec: 1 µs → 100 ms covered. With base-2 buckets
/// that's `log2(100_000_000 ns / 1_000 ns) = log2(100_000) ≈ 17`
/// — we round up to 24 (covers 1 µs to ~16 ms in pow-2 steps,
/// plus a "≥ 16 ms" overflow bucket at index 23). 24 × 4 bytes =
/// 96 bytes of histogram state — well under the 1 KB stack budget
/// the 141 spec calls out.
pub const HISTOGRAM_BUCKETS: usize = 24;

/// Bucket edges in nanoseconds. `BUCKET_EDGES_NS[i]` is the
/// *upper* bound of bucket `i`; sample `ns` lands in bucket `i`
/// where `BUCKET_EDGES_NS[i-1] <= ns < BUCKET_EDGES_NS[i]`
/// (bucket 0 = `[0, 1 µs)`). The last bucket is the overflow
/// "≥ 16.7 ms" catch-all (edge = `u64::MAX`).
pub const BUCKET_EDGES_NS: [u64; HISTOGRAM_BUCKETS] = {
    let mut edges = [0u64; HISTOGRAM_BUCKETS];
    let mut i = 0;
    let mut v: u64 = 1_000; // 1 µs
    while i < HISTOGRAM_BUCKETS - 1 {
        edges[i] = v;
        v = v.saturating_mul(2);
        i += 1;
    }
    edges[HISTOGRAM_BUCKETS - 1] = u64::MAX;
    edges
};

/// Log-distributed histogram for wake-latency samples. Counters
/// are `u32` — at the 256-sample-per-drain ring cap, overflow
/// would require ~16 M cycles of accumulation without ever
/// reading the histogram out. Acceptable for 141.D scenarios
/// which sustain at most a few thousand samples between drains.
#[derive(Debug, Clone)]
pub struct Histogram {
    pub buckets: [u32; HISTOGRAM_BUCKETS],
    /// Total samples accumulated since construction / `clear`.
    /// Sum of `buckets` (kept separately so `clear` is `O(1)`
    /// instead of `O(BUCKETS + 1)`).
    pub total: u32,
}

impl Histogram {
    pub const fn new() -> Self {
        Self {
            buckets: [0; HISTOGRAM_BUCKETS],
            total: 0,
        }
    }

    /// Insert one sample (nanoseconds). Branch-free linear scan;
    /// `HISTOGRAM_BUCKETS = 24` is small enough that a binary
    /// search isn't worth the code-size cost on Cortex-M3.
    #[inline]
    pub fn insert(&mut self, ns: u64) {
        let mut i = 0;
        while i < HISTOGRAM_BUCKETS - 1 {
            if ns < BUCKET_EDGES_NS[i] {
                self.buckets[i] = self.buckets[i].saturating_add(1);
                self.total = self.total.saturating_add(1);
                return;
            }
            i += 1;
        }
        // Fell through every finite edge → overflow bucket.
        self.buckets[HISTOGRAM_BUCKETS - 1] = self.buckets[HISTOGRAM_BUCKETS - 1].saturating_add(1);
        self.total = self.total.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.buckets = [0; HISTOGRAM_BUCKETS];
        self.total = 0;
    }

    /// Compute the percentile bucket edge. Returns `(edge_ns,
    /// rank)` where `edge_ns` is the upper bound of the bucket
    /// the `pct`-th sample lands in. `pct` is `[0, 100]`; values
    /// outside saturate. Useful for on-device P99 reporting
    /// (logging the bucket edge avoids needing the host harness
    /// for a sanity check).
    pub fn percentile(&self, pct: u8) -> Option<u64> {
        if self.total == 0 {
            return None;
        }
        let target = (self.total as u64 * pct.min(100) as u64).div_ceil(100) as u32;
        let mut cumulative: u32 = 0;
        for (i, &count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(count);
            if cumulative >= target {
                return Some(BUCKET_EDGES_NS[i]);
            }
        }
        // Saturation case (rounding); return the last finite edge.
        Some(BUCKET_EDGES_NS[HISTOGRAM_BUCKETS - 1])
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: drain the probe ring into `hist` via a stack
/// buffer of the supplied size. Pass `cycles_to_ns` to map raw
/// cycle deltas to nanoseconds (typically
/// `nros_platform_mps2_an385::timing::cycles_to_ns` partially
/// applied to `SystemCoreClock`). Returns the same
/// `(samples_written, total_writes_since_boot)` tuple [`drain`]
/// does so callers can detect ring wraps.
///
/// `BUF_SAMPLES` should match [`PROBE_SAMPLE_CAP`] for a complete
/// drain per call; smaller buffers are accepted (caller can loop
/// + drain incrementally when memory pressure makes the full
/// cap infeasible, though the 256 * 8 = 2 KB requirement is
/// trivial on the MPS2-AN385).
pub fn drain_into<const BUF_SAMPLES: usize>(
    hist: &mut Histogram,
    cycles_to_ns: impl Fn(u64) -> u64,
) -> (usize, u32) {
    let mut buf = [0u64; BUF_SAMPLES];
    let (n, total) = drain(&mut buf);
    for &cycles in buf.iter().take(n) {
        hist.insert(cycles_to_ns(cycles));
    }
    (n, total)
}

/// Format the histogram as a CSV-shaped string for UART export.
///
/// Format (141.C.1 / .C.2 contract — the host parser at 141.C.2
/// reads this format):
///
/// ```text
/// NROS-WAKE-HIST,v1
/// bucket_edge_ns,count
/// 1000,3
/// 2000,17
/// ...
/// 8388608000,0
/// 18446744073709551615,0
/// total,N
/// END
/// ```
///
/// The `END` sentinel lets the host harness detect the boundary
/// without needing to know `HISTOGRAM_BUCKETS` ahead of time.
/// `bucket_edge_ns` is the bucket's UPPER bound (matches
/// [`BUCKET_EDGES_NS`]); a sample with `ns < edge` lands in that
/// bucket.
///
/// Writer is generic over [`core::fmt::Write`] so embedded
/// callers can pass a UART writer wrapper (`hprintln!`-style or
/// custom) without pulling in `std`.
pub fn write_csv<W: core::fmt::Write>(w: &mut W, hist: &Histogram) -> core::fmt::Result {
    writeln!(w, "NROS-WAKE-HIST,v1")?;
    writeln!(w, "bucket_edge_ns,count")?;
    for (i, &count) in hist.buckets.iter().enumerate() {
        writeln!(w, "{},{}", BUCKET_EDGES_NS[i], count)?;
    }
    writeln!(w, "total,{}", hist.total)?;
    writeln!(w, "END")?;
    Ok(())
}

// ============================================================================
// Host-side parser (Phase 141.C.2) — std only.
// ============================================================================

/// Parse the CSV emitted by [`write_csv`]. Host-only (`std`
/// feature) so the no_std embedded build doesn't pay any cost.
///
/// Returns `(buckets, total)` where `buckets` is a length-
/// [`HISTOGRAM_BUCKETS`] array of `(edge_ns, count)`. Caller can
/// pass through to [`p99_ns`] etc. Returns `Err(&'static str)`
/// when the format doesn't match the v1 contract.
#[cfg(feature = "std")]
pub fn parse_csv(input: &str) -> Result<([(u64, u32); HISTOGRAM_BUCKETS], u32), &'static str> {
    let mut lines = input.lines();
    let header = lines.next().ok_or("empty input")?;
    if header.trim() != "NROS-WAKE-HIST,v1" {
        return Err("missing NROS-WAKE-HIST,v1 header");
    }
    let cols = lines.next().ok_or("missing column header")?;
    if cols.trim() != "bucket_edge_ns,count" {
        return Err("missing bucket_edge_ns,count column header");
    }
    let mut buckets = [(0u64, 0u32); HISTOGRAM_BUCKETS];
    for slot in buckets.iter_mut().take(HISTOGRAM_BUCKETS) {
        let line = lines
            .next()
            .ok_or("histogram truncated before all buckets")?;
        let mut parts = line.split(',');
        let edge: u64 = parts
            .next()
            .ok_or("missing bucket edge")?
            .trim()
            .parse()
            .map_err(|_| "bucket edge not a u64")?;
        let count: u32 = parts
            .next()
            .ok_or("missing bucket count")?
            .trim()
            .parse()
            .map_err(|_| "bucket count not a u32")?;
        *slot = (edge, count);
    }
    let total_line = lines.next().ok_or("missing total line")?;
    let total: u32 = total_line
        .strip_prefix("total,")
        .ok_or("malformed total line")?
        .trim()
        .parse()
        .map_err(|_| "total not a u32")?;
    let end = lines.next().ok_or("missing END sentinel")?;
    if end.trim() != "END" {
        return Err("missing END sentinel");
    }
    Ok((buckets, total))
}

/// Compute the P-th percentile bucket edge from parsed
/// `(edge_ns, count)` data. `pct` is `[0, 100]`. Returns `None`
/// when total is zero.
#[cfg(feature = "std")]
pub fn percentile_ns(buckets: &[(u64, u32); HISTOGRAM_BUCKETS], pct: u8) -> Option<u64> {
    let total: u32 = buckets.iter().map(|(_, c)| *c).sum();
    if total == 0 {
        return None;
    }
    let target = (total as u64 * pct.min(100) as u64).div_ceil(100) as u32;
    let mut cumulative: u32 = 0;
    for &(edge, count) in buckets.iter() {
        cumulative = cumulative.saturating_add(count);
        if cumulative >= target {
            return Some(edge);
        }
    }
    Some(buckets[HISTOGRAM_BUCKETS - 1].0)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn histogram_bucketing_log_distributed() {
        let mut h = Histogram::new();
        h.insert(500); // < 1 µs → bucket 0
        h.insert(1_500); // 1-2 µs → bucket 1
        h.insert(3_000); // 2-4 µs → bucket 2
        h.insert(10_000_000_000); // 10 s → overflow bucket (past
        // edges[22] = ~4.2 s; edges[23] = u64::MAX)
        assert_eq!(h.total, 4);
        assert_eq!(h.buckets[0], 1);
        assert_eq!(h.buckets[1], 1);
        assert_eq!(h.buckets[2], 1);
        assert_eq!(h.buckets[HISTOGRAM_BUCKETS - 1], 1);
    }

    #[test]
    fn percentile_at_99() {
        let mut h = Histogram::new();
        for _ in 0..99 {
            h.insert(500); // 99 samples in bucket 0
        }
        h.insert(1_500); // 1 sample in bucket 1
        // P99 of 100 samples = the 99th sample (1-indexed) =
        // bucket 0 (samples 1..99 land here). edge = 1 µs.
        assert_eq!(h.percentile(99), Some(1_000));
        // P100 lands in bucket 1.
        assert_eq!(h.percentile(100), Some(2_000));
    }

    #[test]
    fn csv_roundtrip() {
        let mut h = Histogram::new();
        h.insert(500);
        h.insert(1_500);
        h.insert(50_000);

        let mut out = std::string::String::new();
        write_csv(&mut out, &h).expect("write");

        let (parsed, total) = parse_csv(&out).expect("parse");
        assert_eq!(total, 3);
        assert_eq!(parsed[0].1, 1); // 500 ns in bucket 0
        assert_eq!(parsed[1].1, 1); // 1500 ns in bucket 1
        // 50_000 ns → log2(50_000 / 1_000) = log2(50) ≈ 5.6 →
        // bucket 6 (edge 64_000).
        assert_eq!(parsed[6].1, 1);
        assert_eq!(percentile_ns(&parsed, 100), Some(64_000));
    }
}

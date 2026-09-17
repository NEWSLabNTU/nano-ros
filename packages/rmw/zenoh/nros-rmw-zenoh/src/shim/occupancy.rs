//! phase-392 amendment B — how much of the payload pools is ever live AT ONCE.
//!
//! # The question this exists to answer
//!
//! `SMALL_PAYLOADS` is `ZPICO_MAX_SUBSCRIBERS x SUBSCRIBER_RING_DEPTH x
//! SUBSCRIBER_BUFFER_SIZE` — every subscriber, every ring slot, every slot
//! full, all at the same instant. That product is what the image RESERVES.
//! Amendment B asks what it ever HOLDS, because an arena sized for the
//! aggregate peak rather than the sum of individual worst cases is a
//! different and possibly larger win than shrinking each pool against its own
//! worst case.
//!
//! The sum is a build fact (`just mem-report` prices it). The aggregate peak
//! is a RUNTIME property of the traffic, so it needs an instrumented run —
//! which is what this module is. It is the same shape as
//! `xPortGetMinimumEverFreeHeapSize` on FreeRTOS and `FreeListHeap::peak()`
//! here: a monotone high-water that only ever moves one way.
//!
//! # Where it samples, and why that instant
//!
//! In [`super::subscriber::subscriber_notify_callback`], which the C shim
//! calls once per arrival IMMEDIATELY AFTER its Release-store to `ring_tail`
//! (`zpico.c`'s ring producer). Ring occupancy only ever RISES at that store
//! and FALLS at the Rust consumer's `consume_head`, so sampling there catches
//! every peak exactly — a sampler on the spin loop would report whatever
//! happened to be live when the executor looked, which is not the number the
//! question wants. Same distinction `nros_zephyr_heap_peak` documents against
//! `nros_zephyr_heap_used`.
//!
//! # Cost, and why this is a feature rather than the unconditional record
//!
//! `SubscriberAllocReport` next door is written unconditionally because it
//! runs ONCE PER SUBSCRIPTION at registration. This one runs once per MESSAGE
//! on the RX path and walks every live subscriber's ring, so it is
//! `O(live_subscribers x SUBSCRIBER_RING_DEPTH)` per arrival. That does not
//! belong in a shipped image, so it is behind `pool-occupancy`, default off.
//! With the feature absent every function here is an empty inline stub and the
//! record does not exist.
//!
//! # What it prints
//!
//! Nothing, until the high-water MOVES. The peak is monotone, so an image
//! converges and goes quiet on its own — one line per new maximum, no periodic
//! chatter, and no call site for the image to remember to add.
//!
//! **That line only appears where an `nros_log` SINK exists**, which is the
//! embedded boot funnels and not a bare native binary — MEASURED on
//! `x86_64-unknown-linux-gnu` during the phase-392 amendment B run, where the
//! record was correct and the announcement went nowhere (the same dispatch-
//! and-drop that issue 0708 fixed for the RTOS families). So [`report`] is
//! not a convenience: it is the portable reader, and a harness on a host
//! should use it rather than grep for the line.

#[cfg(feature = "pool-occupancy")]
pub use imp::*;

/// A snapshot of the payload pools' live-vs-reserved arithmetic.
///
/// Plain `usize`s rather than the record's atomics: a reader wants one
/// consistent set of numbers, not a live view it has to re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PoolOccupancy {
    /// Subscribers that have taken a metadata slot (`NEXT_BUFFER_INDEX`).
    pub subscribers_live: usize,
    /// High-water of `sum over live subscribers of (occupied slots x the
    /// subscriber's payload stride)`. This is what a SLOT-granular arena
    /// would have had to hold at once.
    pub slot_bytes_peak: usize,
    /// High-water of `sum over live subscribers of (the payload BYTES in each
    /// occupied slot)`. The floor of any sharing scheme — it assumes an
    /// allocation sized to the message rather than to the class, which the C
    /// ring producer cannot do today (it writes into a pre-described stride).
    pub exact_bytes_peak: usize,
    /// High-water of the total number of occupied ring slots.
    pub slots_peak: usize,
    /// The deepest any SINGLE ring ever got. `1` means the depth-4 ring never
    /// buffered anything; `SUBSCRIBER_RING_DEPTH` means it reached the drop
    /// edge at least once.
    pub deepest_single_ring: usize,
    /// How many samples found at least one ring FULL. A non-zero count is the
    /// tell that the reserved depth is load-bearing on this traffic, and
    /// therefore that a shared arena would have had to hold it.
    pub full_ring_samples: usize,
    /// Arrivals sampled.
    pub samples: usize,
    /// `SMALL_PAYLOADS + LARGE_PAYLOADS` — the bytes the image RESERVES, the
    /// denominator the peaks above are measured against. A build fact,
    /// carried here so a reader does not have to reconstruct the knobs.
    pub reserved_payload_bytes: usize,
}

#[cfg(not(feature = "pool-occupancy"))]
mod imp_off {
    use super::PoolOccupancy;

    /// Not instrumented: the crate was built without `pool-occupancy`.
    ///
    /// `None` rather than a zeroed snapshot, because a zero peak and an
    /// absent instrument are different facts and a caller that cannot tell
    /// them apart reports "nothing was ever live" about an image that never
    /// looked.
    #[inline]
    pub fn report() -> Option<PoolOccupancy> {
        None
    }

    /// No-op without the feature — the RX path pays nothing.
    #[inline]
    pub(crate) fn sample() {}
}

#[cfg(not(feature = "pool-occupancy"))]
pub use imp_off::report;
#[cfg(not(feature = "pool-occupancy"))]
pub(crate) use imp_off::sample;

#[cfg(feature = "pool-occupancy")]
mod imp {
    use portable_atomic::{AtomicU32, Ordering};

    use super::PoolOccupancy;
    use crate::shim::subscriber::{live_payload, reserved_payload_bytes};

    /// `"POOC"` — pool occupancy. Stored LAST, so a debugger that finds the
    /// magic is looking at a populated record (the `SubscriberAllocReport`
    /// convention next door).
    pub const POOL_OCCUPANCY_MAGIC: u32 = 0x504f4f43;
    /// Layout version for [`PoolOccupancyReport`].
    pub const POOL_OCCUPANCY_VERSION: u32 = 1;

    /// The record, findable by symbol from a debugger or a core dump.
    #[repr(C)]
    pub struct PoolOccupancyReport {
        magic: AtomicU32,
        version: AtomicU32,
        struct_size: AtomicU32,
        subscribers_live: AtomicU32,
        slot_bytes_peak: AtomicU32,
        exact_bytes_peak: AtomicU32,
        slots_peak: AtomicU32,
        deepest_single_ring: AtomicU32,
        full_ring_samples: AtomicU32,
        samples: AtomicU32,
        reserved_payload_bytes: AtomicU32,
    }

    /// See [`PoolOccupancyReport`].
    #[unsafe(no_mangle)]
    #[used]
    pub static NROS_POOL_OCCUPANCY_REPORT: PoolOccupancyReport = PoolOccupancyReport {
        magic: AtomicU32::new(0),
        version: AtomicU32::new(0),
        struct_size: AtomicU32::new(0),
        subscribers_live: AtomicU32::new(0),
        slot_bytes_peak: AtomicU32::new(0),
        exact_bytes_peak: AtomicU32::new(0),
        slots_peak: AtomicU32::new(0),
        deepest_single_ring: AtomicU32::new(0),
        full_ring_samples: AtomicU32::new(0),
        samples: AtomicU32::new(0),
        reserved_payload_bytes: AtomicU32::new(0),
    };

    fn sat(v: usize) -> u32 {
        u32::try_from(v).unwrap_or(u32::MAX)
    }

    /// Sample every live subscriber's ring and fold the totals into the
    /// high-water record. Called from the notify callback, on the producer
    /// thread, after the tail advance.
    pub(crate) fn sample() {
        let live = live_payload();
        let r = &NROS_POOL_OCCUPANCY_REPORT;
        r.version.store(POOL_OCCUPANCY_VERSION, Ordering::Relaxed);
        r.struct_size.store(
            sat(core::mem::size_of::<PoolOccupancyReport>()),
            Ordering::Relaxed,
        );
        r.subscribers_live
            .store(sat(live.subscribers), Ordering::Relaxed);
        r.reserved_payload_bytes
            .store(sat(reserved_payload_bytes()), Ordering::Relaxed);
        r.samples.fetch_add(1, Ordering::Relaxed);
        if live.full_rings > 0 {
            r.full_ring_samples.fetch_add(1, Ordering::Relaxed);
        }
        r.exact_bytes_peak
            .fetch_max(sat(live.exact_bytes), Ordering::Relaxed);
        r.deepest_single_ring
            .fetch_max(sat(live.deepest_ring), Ordering::Relaxed);
        r.slots_peak.fetch_max(sat(live.slots), Ordering::Relaxed);
        // Store the magic LAST, and announce only on a NEW maximum: the peak
        // is monotone, so this converges and the image goes quiet by itself.
        let prev = r
            .slot_bytes_peak
            .fetch_max(sat(live.slot_bytes), Ordering::Relaxed);
        r.magic.store(POOL_OCCUPANCY_MAGIC, Ordering::Relaxed);
        if sat(live.slot_bytes) > prev {
            announce();
        }
    }

    /// One line per new maximum, on the image's own console.
    ///
    /// `nros_log`, not `println!`: a `std::println!` from Rust kills a Zephyr
    /// native_sim image outright (issue 0589), and this instrument is meant to
    /// be usable on exactly the embedded images whose pools are the question.
    fn announce() {
        let s = match report() {
            Some(s) => s,
            None => return,
        };
        nros_log::log_info!(
            nros_log::get_logger("nros-rmw-zenoh"),
            "pool occupancy peak: {} B of {} B reserved ({} slots, deepest ring {}, \
             {} subscribers live) — exact-bytes floor {} B",
            s.slot_bytes_peak,
            s.reserved_payload_bytes,
            s.slots_peak,
            s.deepest_single_ring,
            s.subscribers_live,
            s.exact_bytes_peak
        );
    }

    /// A consistent snapshot of the record, or `None` before the first
    /// arrival (nothing has been sampled, which is not the same fact as a
    /// peak of zero).
    pub fn report() -> Option<PoolOccupancy> {
        let r = &NROS_POOL_OCCUPANCY_REPORT;
        if r.magic.load(Ordering::Relaxed) != POOL_OCCUPANCY_MAGIC {
            return None;
        }
        Some(PoolOccupancy {
            subscribers_live: r.subscribers_live.load(Ordering::Relaxed) as usize,
            slot_bytes_peak: r.slot_bytes_peak.load(Ordering::Relaxed) as usize,
            exact_bytes_peak: r.exact_bytes_peak.load(Ordering::Relaxed) as usize,
            slots_peak: r.slots_peak.load(Ordering::Relaxed) as usize,
            deepest_single_ring: r.deepest_single_ring.load(Ordering::Relaxed) as usize,
            full_ring_samples: r.full_ring_samples.load(Ordering::Relaxed) as usize,
            samples: r.samples.load(Ordering::Relaxed) as usize,
            reserved_payload_bytes: r.reserved_payload_bytes.load(Ordering::Relaxed) as usize,
        })
    }
}

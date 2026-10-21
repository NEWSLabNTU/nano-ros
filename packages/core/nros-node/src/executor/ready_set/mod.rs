//! Phase 110.A — `ReadySet` trait + default `FifoReadySet` impl.
//!
//! `ReadySet` is the queue + selection layer between `spin_once`'s
//! readiness scan, which fills it, and its dispatch loop, which drains it.
//! (Phase 110.A also defined `Activator` and `Dispatcher` traits for those two
//! halves; they were never wired and phase-397 W4 deleted them.)
//!
//! 110.A ships only `FifoReadySet` (registration-order, bit-for-bit
//! parity with the pre-refactor `spin_once`). Phase 110.B adds
//! `EdfReadySet`; phase 110.C adds the bucketed variants.
//!
//! ## Invariants
//!
//! - `insert` is **idempotent** — a second insert for an already-ready
//!   `desc_idx` is a no-op. This matches default ROS 2 behavior:
//!   one ready bit per callback regardless of how many messages are
//!   queued at the rmw layer; the callback drains its rmw queue per
//!   QoS depth itself.
//! - `pop_next` removes the lowest-key entry and returns it.
//! - `clear` empties the set in O(1).
//!
//! ## Capacity
//!
//! The const-generic `N` caps the number of distinct `DescIdx` values
//! the set can track. Phase 110.A holds N = 64 to match the existing
//! `u64` readiness bitmap width; future MAX_HANDLES bumps will widen
//! the storage accordingly (likely `BitSet<N>`).

use super::types::{ActiveJob, DescIdx};

/// Capacity-overflow error returned from [`ReadySet::insert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Overflow;

// Phase 192.8 — the bucketed dispatch (`spin_once`) drives the sets through
// `insert` + `pop_next` only; `clear` / `is_empty` / `contains` are exercised
// solely by this module's unit tests, so they are `#[cfg(test)]` rather than
// kept alive with a blanket `#[allow(dead_code)]`.
pub(crate) trait ReadySet {
    /// Insert a job. Idempotent: a second insert for the same
    /// `desc_idx` returns `Ok(())` without changing internal state.
    fn insert(&mut self, job: ActiveJob) -> Result<(), Overflow>;
    /// Pop the lowest-key job. Returns `None` when the set is empty.
    fn pop_next(&mut self) -> Option<ActiveJob>;
    #[cfg(test)]
    fn clear(&mut self);
    #[cfg(test)]
    fn is_empty(&self) -> bool;
    #[cfg(test)]
    fn contains(&self, desc_idx: DescIdx) -> bool;
}

/// FIFO ready set backed by a 64-bit presence bitmap.
///
/// Selection order is the registration order of `desc_idx` (lowest
/// bit first), which reproduces the pre-110.A `spin_once` behavior
/// exactly.
#[derive(Debug)]
pub(crate) struct FifoReadySet<const N: usize> {
    bits: u64,
}

impl<const N: usize> Default for FifoReadySet<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> FifoReadySet<N> {
    pub const fn new() -> Self {
        // Capacity guard — 110.A bitmap width matches the existing
        // executor scan. Wider sets land with the BitSet rewrite.
        const {
            assert!(N <= 64, "FifoReadySet capacity capped at 64");
        }
        Self { bits: 0 }
    }

    /// No `desc_idx` is queued.
    pub const fn bits_empty(&self) -> bool {
        self.bits == 0
    }

    /// Bulk-set the presence bitmap. Test-only helper (Phase 192.8): the
    /// runtime path inserts per-bit; only the unit tests set the bitmap in
    /// one shot.
    #[cfg(test)]
    pub fn set_bits(&mut self, bits: u64) {
        self.bits = bits;
    }
}

impl<const N: usize> ReadySet for FifoReadySet<N> {
    #[cfg(test)]
    fn clear(&mut self) {
        self.bits = 0;
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.bits == 0
    }

    fn insert(&mut self, job: ActiveJob) -> Result<(), Overflow> {
        let idx = job.desc_idx as usize;
        if idx >= N {
            return Err(Overflow);
        }
        self.bits |= 1u64 << idx;
        Ok(())
    }

    fn pop_next(&mut self) -> Option<ActiveJob> {
        if self.bits == 0 {
            return None;
        }
        let idx = self.bits.trailing_zeros() as DescIdx;
        self.bits &= !(1u64 << idx);
        Some(ActiveJob {
            sort_key: idx as u32,
            desc_idx: idx,
        })
    }

    #[cfg(test)]
    fn contains(&self, desc_idx: DescIdx) -> bool {
        let idx = desc_idx as usize;
        if idx >= 64 {
            return false;
        }
        self.bits & (1u64 << idx) != 0
    }
}

// =============================================================================
// Phase 110.B.a — `EdfReadySet<N>`
// =============================================================================

/// Earliest-Deadline-First ready set backed by a min-heap on
/// `(deadline_us, desc_idx)`.
///
/// Each insert places the job at its deadline-ordered slot;
/// `pop_next` yields the earliest deadline first. Insert is
/// idempotent on `desc_idx` — a second insert for an entry already
/// in the heap returns `Ok(())` without enqueueing a duplicate
/// (matches the FIFO contract; one ready bit per callback regardless
/// of message count).
///
/// Capacity `N` caps the number of distinct entries; tracked via a
/// presence bitmap in addition to the heap so duplicate-detection is
/// O(1).
///
/// Phase 110.B.a (this commit) exposes the type + tests; 110.B.b
/// wires it through `spin_once` once `SchedContext::Edf` callbacks
/// can be bound on Executor.
#[derive(Debug)]
pub(crate) struct EdfReadySet<const N: usize> {
    /// Min-heap on `(deadline_us, desc_idx)`. We keep the heap as a
    /// fixed array + length; insertions sift up, pops sift down.
    heap: [(u32, super::types::DescIdx); N],
    len: usize,
    /// Presence bitmap for O(1) idempotency check on `insert`.
    present: u64,
}

impl<const N: usize> EdfReadySet<N> {
    pub const fn new() -> Self {
        const {
            assert!(N <= 64, "EdfReadySet presence bitmap caps at 64");
        }
        Self {
            heap: [(0, 0); N],
            len: 0,
            present: 0,
        }
    }

    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / 2;
            if self.heap[i].0 < self.heap[parent].0 {
                self.heap.swap(i, parent);
                i = parent;
            } else {
                return;
            }
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            let mut smallest = i;
            if left < self.len && self.heap[left].0 < self.heap[smallest].0 {
                smallest = left;
            }
            if right < self.len && self.heap[right].0 < self.heap[smallest].0 {
                smallest = right;
            }
            if smallest == i {
                return;
            }
            self.heap.swap(i, smallest);
            i = smallest;
        }
    }
}

impl<const N: usize> Default for EdfReadySet<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ReadySet for EdfReadySet<N> {
    #[cfg(test)]
    fn clear(&mut self) {
        self.len = 0;
        self.present = 0;
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn insert(&mut self, job: super::types::ActiveJob) -> Result<(), Overflow> {
        let idx = job.desc_idx as usize;
        if idx >= N || self.len >= N {
            return Err(Overflow);
        }
        // Idempotent: skip when already in the heap.
        if self.present & (1u64 << idx) != 0 {
            return Ok(());
        }
        self.heap[self.len] = (job.sort_key, job.desc_idx);
        self.len += 1;
        self.present |= 1u64 << idx;
        self.sift_up(self.len - 1);
        Ok(())
    }

    fn pop_next(&mut self) -> Option<super::types::ActiveJob> {
        if self.len == 0 {
            return None;
        }
        let (key, desc_idx) = self.heap[0];
        self.len -= 1;
        if self.len > 0 {
            self.heap[0] = self.heap[self.len];
            self.sift_down(0);
        }
        self.present &= !(1u64 << desc_idx as usize);
        Some(super::types::ActiveJob {
            sort_key: key,
            desc_idx,
        })
    }

    #[cfg(test)]
    fn contains(&self, desc_idx: super::types::DescIdx) -> bool {
        let idx = desc_idx as usize;
        if idx >= 64 {
            return false;
        }
        self.present & (1u64 << idx) != 0
    }
}

// =============================================================================
// Phase 110.C — `BucketedFifoSet<NB, N>` + `BucketedEdfSet<NB, N>`
// =============================================================================
//
// Splits a ready set across `NB` priority buckets (defaults to 3:
// Critical / Normal / BestEffort). Within each bucket the underlying
// FIFO bitmap or EDF heap behaves exactly as the un-bucketed variant.
// phase-509 P2 -- the FIFO half is now `RankedFifoSet`, ordered by dispatch
// level (the bucket's reserved level, or `1 + rank` inside `Normal`).
//
// Selection: `pop_next` walks buckets ascending (lowest index first =
// highest priority) and returns the first available job. This gives
// strict static priority across buckets while preserving FIFO /
// EDF ordering inside a bucket.
//
// Single-thread blocking note: an in-flight callback in a lower-
// priority bucket blocks higher-priority work that becomes ready
// during dispatch. For hard-RT preemption see Phase 110.D.

/// phase-509 P2 (RFC-0108 D2) -- the FIFO half of the ready set, ordered by
/// dispatch level: one FIFO per level plus a presence word, so the pick is a
/// count of trailing zeros. A level's FIFO is a bitmap over `desc_idx`, popped
/// lowest bit first, so equal levels keep registration order exactly as the
/// three-bucket set this replaces did.
///
/// It replaced `BucketedFifoSet<3, N>`, whose bucket was the sched context's
/// `Priority` alone: every callback of one tier sat in `Normal` and ran in
/// registration order, whatever rank the contract gave its node. The level is
/// [`Priority::dispatch_level`](super::sched_context::Priority::dispatch_level),
/// which keeps the three buckets as three reserved levels.
#[derive(Debug)]
pub(crate) struct RankedFifoSet<const N: usize> {
    levels: [FifoReadySet<N>; super::sched_context::DISPATCH_LEVELS],
    present: u32,
}

impl<const N: usize> RankedFifoSet<N> {
    pub const fn new() -> Self {
        const {
            assert!(
                super::sched_context::DISPATCH_LEVELS <= 32,
                "the presence word is a u32"
            );
        }
        Self {
            levels: [const { FifoReadySet::<N>::new() }; super::sched_context::DISPATCH_LEVELS],
            present: 0,
        }
    }

    /// Queue `job` at `level` (0 dispatches first). Idempotent per
    /// `desc_idx`, as the level's FIFO is.
    pub fn insert_at(&mut self, level: u8, job: super::types::ActiveJob) -> Result<(), Overflow> {
        let l = level as usize;
        if l >= super::sched_context::DISPATCH_LEVELS {
            return Err(Overflow);
        }
        self.levels[l].insert(job)?;
        self.present |= 1u32 << l;
        Ok(())
    }

    /// Pop the most urgent job whose level is in `lo..hi`: the lowest level
    /// present there, and inside it the lowest `desc_idx`.
    pub fn pop_in(&mut self, lo: u8, hi: u8) -> Option<super::types::ActiveJob> {
        let below_hi = if hi as usize >= 32 {
            u32::MAX
        } else {
            (1u32 << hi) - 1
        };
        let from_lo = if lo as usize >= 32 { 0 } else { u32::MAX << lo };
        let window = self.present & below_hi & from_lo;
        if window == 0 {
            return None;
        }
        let l = window.trailing_zeros() as usize;
        let job = self.levels[l].pop_next();
        if self.levels[l].bits_empty() {
            self.present &= !(1u32 << l);
        }
        job
    }

    /// Pop the most urgent job at any level.
    #[cfg(test)]
    pub fn pop_next(&mut self) -> Option<super::types::ActiveJob> {
        self.pop_in(0, super::sched_context::DISPATCH_LEVELS as u8)
    }
}

impl<const N: usize> Default for RankedFifoSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub(crate) struct BucketedEdfSet<const NB: usize, const N: usize> {
    buckets: [EdfReadySet<N>; NB],
}

impl<const NB: usize, const N: usize> BucketedEdfSet<NB, N> {
    pub const fn new() -> Self {
        const {
            assert!(NB >= 1, "BucketedEdfSet needs at least 1 bucket");
        }
        Self {
            buckets: [const { EdfReadySet::<N>::new() }; NB],
        }
    }

    pub fn insert_into(
        &mut self,
        bucket: usize,
        job: super::types::ActiveJob,
    ) -> Result<(), Overflow> {
        if bucket >= NB {
            return Err(Overflow);
        }
        self.buckets[bucket].insert(job)
    }

    /// Pop a job from a specific bucket, ignoring other buckets. The
    /// dispatcher walks buckets in priority order itself (see `spin_once`),
    /// so the bucketed set only exposes per-bucket draining.
    pub fn pop_from(&mut self, bucket: usize) -> Option<super::types::ActiveJob> {
        self.buckets.get_mut(bucket)?.pop_next()
    }
}

impl<const NB: usize, const N: usize> Default for BucketedEdfSet<NB, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::{vec, vec::Vec};

    #[test]
    fn fifo_empty_after_new() {
        let r: FifoReadySet<64> = FifoReadySet::new();
        assert!(r.is_empty());
    }

    #[test]
    fn fifo_insert_idempotent() {
        let mut r: FifoReadySet<64> = FifoReadySet::new();
        let job = ActiveJob {
            sort_key: 7,
            desc_idx: 7,
        };
        assert!(r.insert(job).is_ok());
        assert!(r.insert(job).is_ok());
        assert!(r.contains(7));
        // pop_next yields exactly one entry — second insert was no-op.
        assert!(r.pop_next().is_some());
        assert!(r.pop_next().is_none());
    }

    #[test]
    fn fifo_pop_lowest_first() {
        let mut r: FifoReadySet<64> = FifoReadySet::new();
        for idx in [3u8, 0u8, 5u8, 1u8] {
            r.insert(ActiveJob {
                sort_key: idx as u32,
                desc_idx: idx,
            })
            .unwrap();
        }
        let popped: Vec<u8> = core::iter::from_fn(|| r.pop_next().map(|j| j.desc_idx)).collect();
        assert_eq!(popped, vec![0, 1, 3, 5]);
    }

    #[test]
    fn fifo_overflow_rejected() {
        let mut r: FifoReadySet<8> = FifoReadySet::new();
        let res = r.insert(ActiveJob {
            sort_key: 9,
            desc_idx: 9,
        });
        assert_eq!(res, Err(Overflow));
    }

    #[test]
    fn fifo_set_bits_round_trip() {
        let mut r: FifoReadySet<64> = FifoReadySet::new();
        r.set_bits(0b10110);
        assert_eq!(r.pop_next().unwrap().desc_idx, 1);
        assert_eq!(r.pop_next().unwrap().desc_idx, 2);
        assert_eq!(r.pop_next().unwrap().desc_idx, 4);
        assert!(r.pop_next().is_none());
    }

    fn job(idx: u8) -> ActiveJob {
        ActiveJob {
            sort_key: idx as u32,
            desc_idx: idx,
        }
    }

    /// phase-509 P2 -- the lowest level first, and inside a level the lowest
    /// `desc_idx` (registration order).
    #[test]
    fn ranked_pops_by_level_then_registration_order() {
        let mut r: RankedFifoSet<64> = RankedFifoSet::new();
        for (level, idx) in [(3u8, 0u8), (1, 5), (3, 1), (1, 2), (0, 9)] {
            r.insert_at(level, job(idx)).unwrap();
        }
        let popped: Vec<u8> = core::iter::from_fn(|| r.pop_next().map(|j| j.desc_idx)).collect();
        assert_eq!(popped, vec![9, 2, 5, 0, 1]);
    }

    /// phase-509 P2 -- `pop_in` reaches only its window of levels.
    #[test]
    fn ranked_pop_in_stays_inside_its_window() {
        let mut r: RankedFifoSet<64> = RankedFifoSet::new();
        r.insert_at(0, job(1)).unwrap();
        r.insert_at(5, job(2)).unwrap();
        r.insert_at(31, job(3)).unwrap();
        assert_eq!(r.pop_in(1, 31).map(|j| j.desc_idx), Some(2));
        assert_eq!(r.pop_in(1, 31), None);
        assert_eq!(r.pop_in(31, 32).map(|j| j.desc_idx), Some(3));
        assert_eq!(r.pop_in(0, 1).map(|j| j.desc_idx), Some(1));
        assert!(r.pop_next().is_none());
    }

    /// phase-509 P2 -- one ready bit per callback, as in every set here.
    #[test]
    fn ranked_insert_idempotent_and_bounded() {
        let mut r: RankedFifoSet<8> = RankedFifoSet::new();
        r.insert_at(2, job(4)).unwrap();
        r.insert_at(2, job(4)).unwrap();
        assert_eq!(r.insert_at(32, job(1)), Err(Overflow));
        assert_eq!(r.insert_at(0, job(9)), Err(Overflow));
        assert!(r.pop_next().is_some());
        assert!(r.pop_next().is_none());
    }

    #[test]
    fn edf_pop_earliest_deadline_first() {
        let mut r: EdfReadySet<8> = EdfReadySet::new();
        for (deadline, idx) in [(50u32, 3u8), (10, 1), (30, 5), (20, 0)] {
            r.insert(ActiveJob {
                sort_key: deadline,
                desc_idx: idx,
            })
            .unwrap();
        }
        let popped: Vec<u32> = core::iter::from_fn(|| r.pop_next().map(|j| j.sort_key)).collect();
        assert_eq!(popped, vec![10, 20, 30, 50]);
    }

    #[test]
    fn edf_insert_idempotent() {
        let mut r: EdfReadySet<8> = EdfReadySet::new();
        let job = ActiveJob {
            sort_key: 100,
            desc_idx: 2,
        };
        assert!(r.insert(job).is_ok());
        // Second insert for the same desc_idx is a no-op even when
        // sort_key differs (callback already queued — message-burst
        // semantics handled by the rmw drain).
        let dup = ActiveJob {
            sort_key: 50,
            desc_idx: 2,
        };
        assert!(r.insert(dup).is_ok());
        assert!(r.contains(2));
        let first = r.pop_next().unwrap();
        assert_eq!(first.desc_idx, 2);
        assert_eq!(first.sort_key, 100);
        assert!(r.pop_next().is_none());
    }

    #[test]
    fn edf_overflow_rejected() {
        let mut r: EdfReadySet<2> = EdfReadySet::new();
        r.insert(ActiveJob {
            sort_key: 1,
            desc_idx: 0,
        })
        .unwrap();
        r.insert(ActiveJob {
            sort_key: 2,
            desc_idx: 1,
        })
        .unwrap();
        let res = r.insert(ActiveJob {
            sort_key: 3,
            desc_idx: 2,
        });
        assert_eq!(res, Err(Overflow));
    }

    #[test]
    fn edf_clear_resets_state() {
        let mut r: EdfReadySet<8> = EdfReadySet::new();
        for idx in 0..4u8 {
            r.insert(ActiveJob {
                sort_key: 100 - idx as u32,
                desc_idx: idx,
            })
            .unwrap();
        }
        assert!(!r.is_empty());
        r.clear();
        assert!(r.is_empty());
        for idx in 0..4u8 {
            assert!(!r.contains(idx));
        }
    }
}

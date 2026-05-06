//! Phase 110.A — `ReadySet` trait + default `FifoReadySet` impl.
//!
//! `ReadySet` abstracts the queue + selection layer between
//! [`Activator`](super::activator::Activator) and
//! [`Dispatcher`](super::dispatcher::Dispatcher).
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
#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
pub(crate) struct Overflow;

// `clear` / `is_empty` / `insert` / `contains` are wired by the
// EDF + bucketed dispatchers (110.B / 110.C); 110.A only exercises
// `pop_next` from `spin_once`. Marked `dead_code` until then.
#[allow(dead_code)]
pub(crate) trait ReadySet {
    fn clear(&mut self);
    fn is_empty(&self) -> bool;
    /// Insert a job. Idempotent: a second insert for the same
    /// `desc_idx` returns `Ok(())` without changing internal state.
    fn insert(&mut self, job: ActiveJob) -> Result<(), Overflow>;
    /// Pop the lowest-key job. Returns `None` when the set is empty.
    fn pop_next(&mut self) -> Option<ActiveJob>;
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

    /// Bulk-set the presence bitmap. Used by the default
    /// [`Activator`](super::activator::Activator) impl which produces
    /// a full `u64` mask in one pass and writes it through.
    pub fn set_bits(&mut self, bits: u64) {
        self.bits = bits;
    }

    /// Read the raw bitmap. Internal use only — the dispatcher walks
    /// the set via `pop_next`.
    #[allow(dead_code)]
    pub fn bits(&self) -> u64 {
        self.bits
    }
}

impl<const N: usize> ReadySet for FifoReadySet<N> {
    fn clear(&mut self) {
        self.bits = 0;
    }

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
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b spin_once dispatch.
#[derive(Debug)]
pub(crate) struct EdfReadySet<const N: usize> {
    /// Min-heap on `(deadline_us, desc_idx)`. We keep the heap as a
    /// fixed array + length; insertions sift up, pops sift down.
    heap: [(u32, super::types::DescIdx); N],
    len: usize,
    /// Presence bitmap for O(1) idempotency check on `insert`.
    present: u64,
}

#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b spin_once dispatch.
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

#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b spin_once dispatch.
impl<const N: usize> Default for EdfReadySet<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ReadySet for EdfReadySet<N> {
    fn clear(&mut self) {
        self.len = 0;
        self.present = 0;
    }

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

    fn contains(&self, desc_idx: super::types::DescIdx) -> bool {
        let idx = desc_idx as usize;
        if idx >= 64 {
            return false;
        }
        self.present & (1u64 << idx) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::vec::Vec;
    use alloc::vec;

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

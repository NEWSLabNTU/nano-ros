//! The shim's bump allocators: ONE spelling of "claim the next slot of a
//! static table" and "hand it back after a later step failed" (issue 1956).
//!
//! Every table here (subscriber metadata, the two payload classes, service
//! headers, the service inbox rings) is a cursor over a fixed array, and slots
//! are never freed one by one. Each site used to claim with `fetch_add(1)` and
//! undo with `fetch_sub(1)`, which is right only when nobody claimed in
//! between. When somebody did — two tiers declaring at once, one of them
//! failing — the undo moved the cursor back BELOW a slot that is live, so the
//! next claim handed out that slot a second time. For a subscriber that means
//! two subscriptions over one SPSC ring: two producers, two consumers, and
//! `ring_head` can then pass `ring_tail`, after which the ring reads as
//! non-empty forever and as full to every new sample.
//!
//! The rule now:
//! - [`claim`] never moves a cursor past its table, so a refusal needs no undo.
//! - [`release`] undoes a claim only while it is still the LATEST one. If a
//!   later claim has landed, the slot stays spent: leaking one slot of a table
//!   is a capacity loss, handing it out twice is memory corruption.

use portable_atomic::{AtomicUsize, Ordering};

/// Claim the next slot below `cap`, or `None` when the table is spent.
pub(super) fn claim(cursor: &AtomicUsize, cap: usize) -> Option<usize> {
    cursor
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            (n < cap).then_some(n + 1)
        })
        .ok()
}

/// Hand back `index`, claimed from `cursor` by [`claim`], after a later step
/// of the same creation failed. Returns whether the slot is reusable; `false`
/// means a later claim landed first and the slot stays spent.
pub(super) fn release(cursor: &AtomicUsize, index: usize) -> bool {
    cursor
        .compare_exchange(index + 1, index, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_stops_at_the_table_and_needs_no_undo() {
        let cursor = AtomicUsize::new(0);
        assert_eq!(claim(&cursor, 2), Some(0));
        assert_eq!(claim(&cursor, 2), Some(1));
        assert_eq!(claim(&cursor, 2), None);
        assert_eq!(claim(&cursor, 2), None);
        // A refusal left the cursor AT the table, not past it.
        assert_eq!(cursor.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn release_of_the_latest_claim_makes_it_reusable() {
        let cursor = AtomicUsize::new(0);
        let a = claim(&cursor, 4).unwrap();
        assert!(release(&cursor, a));
        assert_eq!(claim(&cursor, 4), Some(a));
    }

    /// Issue 1956's interleaving: A claims, B claims, A fails and releases.
    /// The old `fetch_sub` moved the cursor back to B's slot, so the next
    /// claim returned B's index while B was live.
    #[test]
    fn release_after_a_later_claim_never_hands_a_live_slot_out_twice() {
        let cursor = AtomicUsize::new(0);
        let a = claim(&cursor, 4).unwrap();
        let b = claim(&cursor, 4).unwrap();
        assert!(!release(&cursor, a));
        let c = claim(&cursor, 4).unwrap();
        assert_ne!(c, b, "slot {b} is live and was handed out again");
        assert_ne!(
            c, a,
            "slot {a} could not be undone and was handed out again"
        );
    }

    #[test]
    fn concurrent_claims_with_failures_never_duplicate_a_live_slot() {
        extern crate std;
        use std::{sync::Mutex, vec::Vec};

        const CAP: usize = 4096;
        static CURSOR: AtomicUsize = AtomicUsize::new(0);
        static LIVE: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let threads: Vec<_> = (0..8)
            .map(|t| {
                std::thread::spawn(move || {
                    for i in 0..400 {
                        let Some(idx) = claim(&CURSOR, CAP) else {
                            return;
                        };
                        // Every third creation fails after its claim.
                        if (t + i) % 3 == 0 {
                            release(&CURSOR, idx);
                        } else {
                            LIVE.lock().unwrap().push(idx);
                        }
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let mut live = LIVE.lock().unwrap().clone();
        let n = live.len();
        live.sort_unstable();
        live.dedup();
        assert_eq!(live.len(), n, "a live slot was handed out twice");
    }
}

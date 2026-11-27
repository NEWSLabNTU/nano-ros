// RFC-0038 D5, phase-579 W3p; the classes are D1's; the placement hook is
// phase-574 W3's; the owned handle over these indices is RFC-0139 D6 (W3b).

//! The core slot pool: fixed-size slots a caller claims, hands on, and
//! releases wherever its use ends.
//!
//! A device job takes its input in a slot claimed here and gives it back when
//! the job is done, which may be in the device's completion interrupt. So the
//! pool is:
//!
//! - **in core**, depending on no RMW backend, because a device is not RMW
//!   traffic;
//! - **two size classes**, `small` and `large`, each a fixed
//!   slot size and a slot count, both build knobs
//!   (`NROS_SLOT_{SMALL,LARGE}_{SLOT_BYTES,COUNT}`). A class whose count is 0,
//!   the default, claims nothing and reserves nothing;
//! - **lock-free**: claim and release from any thread with no lock, and
//!   release also from an interrupt handler. A class's ownership is a bitmap,
//!   one bit per slot, changed only by atomic read-modify-write. A bit names a
//!   slot, so a stale read can never hand one slot out twice (there is no ABA:
//!   nothing is reused but the bit itself). A claim makes at most
//!   [`CLAIM_ATTEMPTS_PER_WORD`] attempts on one word before it moves on;
//! - **refusing on exhaustion**: a claim with no free slot returns `None`. It
//!   never blocks, never waits for a release and never allocates, and the class
//!   counts the refusal ([`SlotPool::refusals`]), so an image sized too small
//!   says so;
//! - **a `.bss` static** per class ([`SMALL`], [`LARGE`]), until a board can
//!   place a pool in a memory region it chooses.
//!
//! This module is the allocator only. What a slot holds and how ownership
//! moves between threads (the owned handle, released on drop) is built on
//! these indices.
//!
//! On a core without a native compare-and-swap (ARMv6-M) the atomics come from
//! `portable_atomic`, which makes each one a short critical section with
//! interrupts masked: still safe from an interrupt handler, and still no lock
//! a thread can hold across a preemption.

use core::{cell::UnsafeCell, mem::MaybeUninit};

use portable_atomic::{AtomicU32, Ordering};

use crate::config::{
    SLOT_LARGE_COUNT, SLOT_LARGE_SLOT_BYTES, SLOT_SMALL_COUNT, SLOT_SMALL_SLOT_BYTES,
};

/// How many compare-and-swap attempts a claim makes on one bitmap word before
/// it moves to the next. Bounded, so a claim under contention finishes in a
/// bounded number of steps; a claim that loses every race on every word is
/// refused like an exhausted one.
pub const CLAIM_ATTEMPTS_PER_WORD: usize = 4;

/// The number of 32-bit bitmap words a class of `count` slots needs.
#[must_use]
pub const fn bitmap_words(count: usize) -> usize {
    count.div_ceil(32)
}

/// Which of the two size classes a slot comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotClass {
    /// [`SMALL`]: `NROS_SLOT_SMALL_SLOT_BYTES` each.
    Small,
    /// [`LARGE`]: `NROS_SLOT_LARGE_SLOT_BYTES` each.
    Large,
}

/// One claimed slot: its class and its index in that class.
///
/// Plain data, so it is what an owned handle carries. Holding one is not
/// ownership on its own: it is the claimer's to release exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId {
    /// The class the slot belongs to.
    pub class: SlotClass,
    /// The slot's index in its class.
    pub index: usize,
}

/// One size class: `COUNT` slots of `SLOT_BYTES` each, and their ownership
/// bitmap of `WORDS` words (which must be [`bitmap_words`]`(COUNT)`).
pub struct SlotPool<const SLOT_BYTES: usize, const COUNT: usize, const WORDS: usize> {
    /// Bit `i` of word `w` set: slot `32 w + i` is claimed. Bits past `COUNT`
    /// are never set.
    bits: [AtomicU32; WORDS],
    /// Claims refused because no slot was free.
    refusals: AtomicU32,
    storage: [UnsafeCell<MaybeUninit<[u8; SLOT_BYTES]>>; COUNT],
}

// SAFETY: a slot's bytes are reached only through `slot_ptr`, by the one
// caller whose claim set its bit, until that caller releases it. The bitmap
// and the counter are atomics.
unsafe impl<const SLOT_BYTES: usize, const COUNT: usize, const WORDS: usize> Sync
    for SlotPool<SLOT_BYTES, COUNT, WORDS>
{
}

impl<const SLOT_BYTES: usize, const COUNT: usize, const WORDS: usize> Default
    for SlotPool<SLOT_BYTES, COUNT, WORDS>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const SLOT_BYTES: usize, const COUNT: usize, const WORDS: usize>
    SlotPool<SLOT_BYTES, COUNT, WORDS>
{
    /// An empty class, every slot free. `const`, so a class can be a static
    /// (its storage is uninitialised, which places it in `.bss`).
    ///
    /// # Panics
    ///
    /// At compile time for a static, if `WORDS` is not
    /// [`bitmap_words`]`(COUNT)`.
    #[must_use]
    pub const fn new() -> Self {
        assert!(
            WORDS == bitmap_words(COUNT),
            "a slot pool's bitmap must have bitmap_words(COUNT) words"
        );
        Self {
            bits: [const { AtomicU32::new(0) }; WORDS],
            refusals: AtomicU32::new(0),
            storage: [const { UnsafeCell::new(MaybeUninit::uninit()) }; COUNT],
        }
    }

    /// How many slots the class has.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        COUNT
    }

    /// How many bytes each slot holds.
    #[must_use]
    pub const fn slot_bytes(&self) -> usize {
        SLOT_BYTES
    }

    /// The bits of word `w` that name a slot: all 32, except in a last word
    /// that `COUNT` only partly fills.
    const fn valid_mask(w: usize) -> u32 {
        let first = w * 32;
        let left = COUNT - first;
        if left >= 32 {
            u32::MAX
        } else {
            (1u32 << left) - 1
        }
    }

    /// Claim a free slot, or refuse.
    ///
    /// Lock-free and callable from any thread. Returns the slot's index, or
    /// `None` with [`Self::refusals`] counted when no slot was free (or every
    /// attempt lost a race, see [`CLAIM_ATTEMPTS_PER_WORD`]). Never blocks and
    /// never allocates.
    pub fn claim(&self) -> Option<usize> {
        for (w, word) in self.bits.iter().enumerate() {
            let valid = Self::valid_mask(w);
            let mut current = word.load(Ordering::Relaxed);
            for _ in 0..CLAIM_ATTEMPTS_PER_WORD {
                let free = !current & valid;
                if free == 0 {
                    break;
                }
                let bit = free.trailing_zeros();
                // Acquire on success: the previous owner's writes to the slot,
                // published by its Release in `release`, happen before ours.
                match word.compare_exchange_weak(
                    current,
                    current | (1u32 << bit),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return Some(w * 32 + bit as usize),
                    Err(seen) => current = seen,
                }
            }
        }
        self.refusals.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Release slot `index`, claimed earlier and not yet released.
    ///
    /// Lock-free, and safe to call from an interrupt handler. Returns `false`,
    /// changing nothing, for an index out of range or a slot that is not
    /// claimed (a double release), which a debug build also asserts.
    pub fn release(&self, index: usize) -> bool {
        if index >= COUNT {
            debug_assert!(false, "slot {index} is out of range ({COUNT} slots)");
            return false;
        }
        let bit = 1u32 << (index % 32);
        // Release: our writes to the slot happen before the next claimer's
        // Acquire.
        let before = self.bits[index / 32].fetch_and(!bit, Ordering::Release);
        let was_claimed = before & bit != 0;
        debug_assert!(was_claimed, "slot {index} released but not claimed");
        was_claimed
    }

    /// The first byte of slot `index`'s storage.
    ///
    /// Dereferencing it is the claimer's right alone, from its claim to its
    /// release; the pool does not check that the slot is claimed.
    ///
    /// # Panics
    ///
    /// If `index` is out of range.
    #[must_use]
    pub fn slot_ptr(&self, index: usize) -> *mut u8 {
        self.storage[index].get().cast::<u8>()
    }

    /// How many claims this class has refused.
    #[must_use]
    pub fn refusals(&self) -> u32 {
        self.refusals.load(Ordering::Relaxed)
    }

    /// How many slots are claimed now. A snapshot, for diagnostics: it can be
    /// stale the moment it returns.
    #[must_use]
    pub fn in_use(&self) -> usize {
        self.bits
            .iter()
            .map(|w| w.load(Ordering::Relaxed).count_ones() as usize)
            .sum()
    }
}

// nros-pool: SMALL = NROS_SLOT_SMALL_COUNT * NROS_SLOT_SMALL_SLOT_BYTES
/// The `small` class, `NROS_SLOT_SMALL_COUNT` slots of
/// `NROS_SLOT_SMALL_SLOT_BYTES`.
pub static SMALL: SlotPool<
    SLOT_SMALL_SLOT_BYTES,
    SLOT_SMALL_COUNT,
    { bitmap_words(SLOT_SMALL_COUNT) },
> = SlotPool::new();

// nros-pool: LARGE = NROS_SLOT_LARGE_COUNT * NROS_SLOT_LARGE_SLOT_BYTES
/// The `large` class, `NROS_SLOT_LARGE_COUNT` slots of
/// `NROS_SLOT_LARGE_SLOT_BYTES`.
pub static LARGE: SlotPool<
    SLOT_LARGE_SLOT_BYTES,
    SLOT_LARGE_COUNT,
    { bitmap_words(SLOT_LARGE_COUNT) },
> = SlotPool::new();

/// Claim a slot of `class` from the image's pool, or refuse (counted on that
/// class). See [`SlotPool::claim`].
pub fn claim(class: SlotClass) -> Option<SlotId> {
    let index = match class {
        SlotClass::Small => SMALL.claim(),
        SlotClass::Large => LARGE.claim(),
    }?;
    Some(SlotId { class, index })
}

/// Release a slot claimed by [`claim`]. Safe from an interrupt handler. See
/// [`SlotPool::release`].
pub fn release(slot: SlotId) -> bool {
    match slot.class {
        SlotClass::Small => SMALL.release(slot.index),
        SlotClass::Large => LARGE.release(slot.index),
    }
}

/// The first byte of a claimed slot, and its size. See
/// [`SlotPool::slot_ptr`]: dereferencing it is the claimer's right alone.
#[must_use]
pub fn slot_bytes(slot: SlotId) -> (*mut u8, usize) {
    match slot.class {
        SlotClass::Small => (SMALL.slot_ptr(slot.index), SMALL.slot_bytes()),
        SlotClass::Large => (LARGE.slot_ptr(slot.index), LARGE.slot_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread, vec::Vec};

    #[test]
    fn claims_every_slot_then_refuses_and_counts() {
        let pool: SlotPool<8, 40, { bitmap_words(40) }> = SlotPool::new();
        let mut got: Vec<usize> = (0..40)
            .map(|_| pool.claim().expect("a free slot"))
            .collect();
        got.sort_unstable();
        assert_eq!(got, (0..40).collect::<Vec<_>>(), "each slot once");
        assert_eq!(pool.in_use(), 40);
        assert_eq!(pool.claim(), None, "exhausted refuses");
        assert_eq!(pool.claim(), None);
        assert_eq!(pool.refusals(), 2, "each refusal is counted");
    }

    #[test]
    fn a_released_slot_is_claimed_again() {
        let pool: SlotPool<8, 3, 1> = SlotPool::new();
        let a = pool.claim().unwrap();
        let b = pool.claim().unwrap();
        let c = pool.claim().unwrap();
        assert_eq!(pool.claim(), None);
        assert!(pool.release(b));
        assert_eq!(pool.claim(), Some(b), "the one free slot");
        assert!(pool.release(a) && pool.release(b) && pool.release(c));
        assert_eq!(pool.in_use(), 0);
    }

    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "released but not claimed"))]
    fn a_double_release_changes_nothing() {
        let pool: SlotPool<8, 2, 1> = SlotPool::new();
        let a = pool.claim().unwrap();
        assert!(pool.release(a));
        // Release builds: refused, and the bitmap is unchanged.
        assert!(!pool.release(a));
        assert_eq!(pool.in_use(), 0);
    }

    #[test]
    fn a_zero_count_class_claims_nothing() {
        let pool: SlotPool<8, 0, 0> = SlotPool::new();
        assert_eq!(pool.claim(), None);
        assert_eq!(pool.refusals(), 1);
        assert_eq!(
            core::mem::size_of_val(&pool.storage),
            0,
            "and reserves nothing"
        );
    }

    #[test]
    fn the_default_image_reserves_no_slot() {
        // The knobs default to a count of 0 in both classes.
        assert_eq!(SLOT_SMALL_COUNT, 0);
        assert_eq!(SLOT_LARGE_COUNT, 0);
        assert_eq!(claim(SlotClass::Small), None);
        assert_eq!(claim(SlotClass::Large), None);
    }

    #[test]
    fn a_slot_holds_what_its_claimer_wrote() {
        let pool: SlotPool<16, 2, 1> = SlotPool::new();
        let a = pool.claim().unwrap();
        let b = pool.claim().unwrap();
        // SAFETY: each slot is ours from its claim to its release.
        unsafe {
            core::ptr::write_bytes(pool.slot_ptr(a), 0xA5, 16);
            core::ptr::write_bytes(pool.slot_ptr(b), 0x5A, 16);
            assert!(
                core::slice::from_raw_parts(pool.slot_ptr(a), 16)
                    .iter()
                    .all(|&x| x == 0xA5)
            );
        }
        assert!(pool.release(a) && pool.release(b));
    }

    /// Several threads claim, write their mark, check it, and release, as
    /// fast as they can. A slot handed to two holders at once is caught by the
    /// per-slot holder count; a lost slot is caught by the bitmap at the end.
    #[test]
    fn concurrent_claim_and_release_lose_and_duplicate_nothing() {
        const SLOTS: usize = 70;
        const THREADS: usize = 8;
        const ROUNDS: usize = 20_000;
        let pool: Arc<SlotPool<8, SLOTS, { bitmap_words(SLOTS) }>> = Arc::new(SlotPool::new());
        let holders: Arc<Vec<AtomicU32>> =
            Arc::new((0..SLOTS).map(|_| AtomicU32::new(0)).collect());
        let claimed = Arc::new(AtomicU32::new(0));
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let (pool, holders, claimed) = (pool.clone(), holders.clone(), claimed.clone());
                thread::spawn(move || {
                    let mark = 0x10 + t as u8;
                    let mut held: Vec<usize> = Vec::new();
                    for round in 0..ROUNDS {
                        if let Some(i) = pool.claim() {
                            let before = holders[i].fetch_add(1, Ordering::AcqRel);
                            assert_eq!(before, 0, "slot {i} handed to two holders");
                            // SAFETY: ours from claim to release.
                            unsafe { core::ptr::write_bytes(pool.slot_ptr(i), mark, 8) };
                            held.push(i);
                            claimed.fetch_add(1, Ordering::Relaxed);
                        }
                        // Hold a few at a time, so the pool also runs dry.
                        if held.len() > 12 || (round % 7 == 0 && !held.is_empty()) {
                            let i = held.remove(round % held.len());
                            // SAFETY: still ours.
                            let ok = unsafe {
                                core::slice::from_raw_parts(pool.slot_ptr(i), 8)
                                    .iter()
                                    .all(|&x| x == mark)
                            };
                            assert!(ok, "slot {i} was written by another holder");
                            holders[i].fetch_sub(1, Ordering::AcqRel);
                            assert!(pool.release(i), "slot {i} was not claimed");
                        }
                    }
                    for i in held {
                        holders[i].fetch_sub(1, Ordering::AcqRel);
                        assert!(pool.release(i));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("a worker failed");
        }
        assert_eq!(pool.in_use(), 0, "every claimed slot came back");
        assert!(holders.iter().all(|h| h.load(Ordering::Relaxed) == 0));
        assert!(
            claimed.load(Ordering::Relaxed) > 0,
            "the stress claimed something"
        );
        // Every slot is still claimable: none was lost.
        let mut all: Vec<usize> = (0..SLOTS).map(|_| pool.claim().expect("a slot")).collect();
        all.sort_unstable();
        assert_eq!(all, (0..SLOTS).collect::<Vec<_>>());
    }
}

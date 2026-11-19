//! One bit per callback slot, over as many `u64` words as the image needs.
//!
//! The executor names a set of callback slots (ready, attributed, deferred,
//! in a band, ...) as a [`SlotMask`]: `crate::config::SLOT_WORDS` words, sized
//! from the image, so the most callbacks one executor holds is
//! `crate::config::CALLBACK_SLOT_CAPACITY`. An image of 64 callbacks or fewer
//! has one word, the layout the masks had as a bare `u64`.
//!
//! [`AtomicSlotMask`] is the cross-thread twin, written by a backend's arrival
//! path (and an interrupt's): one `usize` atomic per machine word, so each
//! update stays a single-word, lock-free read-modify-write and a target with
//! no 64-bit atomics keeps the `portable_atomic` fallback it had.

// Issue 2541 -- before this the masks were one `u64` and the table could not
// pass 64 callbacks.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use crate::config::{CALLBACK_SLOT_CAPACITY, SLOT_WORDS};

/// A set of callback slots, one bit per slot, `SLOT_WORDS` words of 64.
///
/// An index at or above the capacity is outside every mask: [`Self::bit`] of
/// it is empty, [`Self::test`] of it is `false`, and setting it does nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SlotMask(pub(crate) [u64; SLOT_WORDS]);

impl Default for SlotMask {
    #[inline]
    fn default() -> Self {
        Self::EMPTY
    }
}

#[allow(dead_code)] // each build uses the subset its features reach
impl SlotMask {
    /// No slot.
    pub(crate) const EMPTY: Self = Self([0; SLOT_WORDS]);
    /// Every slot up to the capacity (which is a whole number of words).
    pub(crate) const ALL: Self = Self([u64::MAX; SLOT_WORDS]);
    /// Slots one mask can name.
    pub(crate) const CAPACITY: usize = CALLBACK_SLOT_CAPACITY;

    /// The mask holding `slot` alone; empty when `slot` is past the capacity.
    #[inline]
    pub(crate) const fn bit(slot: usize) -> Self {
        let mut m = Self::EMPTY;
        if slot < CALLBACK_SLOT_CAPACITY {
            m.0[slot / 64] = 1u64 << (slot % 64);
        }
        m
    }

    /// The mask whose first word is `word` (slots 0..64) and every other
    /// word empty.
    #[inline]
    pub(crate) const fn from_word0(word: u64) -> Self {
        let mut m = Self::EMPTY;
        m.0[0] = word;
        m
    }

    /// Word `k` (slots `64 * k .. 64 * k + 64`); 0 past the last word.
    #[inline]
    pub(crate) const fn word(self, k: usize) -> u64 {
        if k < SLOT_WORDS { self.0[k] } else { 0 }
    }

    /// `self` with `slot` added.
    #[inline]
    pub(crate) const fn with(self, slot: usize) -> Self {
        self.union(Self::bit(slot))
    }

    /// Add `slot`.
    #[inline]
    pub(crate) fn set(&mut self, slot: usize) {
        if slot < CALLBACK_SLOT_CAPACITY {
            self.0[slot / 64] |= 1u64 << (slot % 64);
        }
    }

    /// Remove `slot`.
    #[inline]
    pub(crate) fn clear(&mut self, slot: usize) {
        if slot < CALLBACK_SLOT_CAPACITY {
            self.0[slot / 64] &= !(1u64 << (slot % 64));
        }
    }

    /// Whether `slot` is in the mask.
    #[inline]
    pub(crate) const fn test(self, slot: usize) -> bool {
        slot < CALLBACK_SLOT_CAPACITY && self.0[slot / 64] & (1u64 << (slot % 64)) != 0
    }

    /// No slot is in the mask.
    #[inline]
    pub(crate) const fn is_empty(self) -> bool {
        let mut k = 0;
        while k < SLOT_WORDS {
            if self.0[k] != 0 {
                return false;
            }
            k += 1;
        }
        true
    }

    /// Slots in either.
    #[inline]
    pub(crate) const fn union(self, other: Self) -> Self {
        let mut m = self;
        let mut k = 0;
        while k < SLOT_WORDS {
            m.0[k] |= other.0[k];
            k += 1;
        }
        m
    }

    /// Slots in both.
    #[inline]
    pub(crate) const fn intersection(self, other: Self) -> Self {
        let mut m = self;
        let mut k = 0;
        while k < SLOT_WORDS {
            m.0[k] &= other.0[k];
            k += 1;
        }
        m
    }

    /// Slots in `self` and not in `other`.
    #[inline]
    pub(crate) const fn and_not(self, other: Self) -> Self {
        let mut m = self;
        let mut k = 0;
        while k < SLOT_WORDS {
            m.0[k] &= !other.0[k];
            k += 1;
        }
        m
    }

    /// Every slot not in `self`.
    #[inline]
    pub(crate) const fn complement(self) -> Self {
        Self::ALL.and_not(self)
    }

    /// Some slot is in both.
    #[inline]
    pub(crate) const fn intersects(self, other: Self) -> bool {
        !self.intersection(other).is_empty()
    }

    /// Every slot of `other` is in `self`.
    #[inline]
    pub(crate) const fn contains_all(self, other: Self) -> bool {
        other.and_not(self).is_empty()
    }

    /// Slots in the mask.
    #[inline]
    pub(crate) const fn count(self) -> u32 {
        let mut n = 0;
        let mut k = 0;
        while k < SLOT_WORDS {
            n += self.0[k].count_ones();
            k += 1;
        }
        n
    }

    /// The lowest slot in the mask.
    #[inline]
    pub(crate) const fn first_set(self) -> Option<usize> {
        let mut k = 0;
        while k < SLOT_WORDS {
            if self.0[k] != 0 {
                return Some(k * 64 + self.0[k].trailing_zeros() as usize);
            }
            k += 1;
        }
        None
    }

    /// The slots in the mask, lowest first.
    #[inline]
    pub(crate) fn iter_ones(self) -> IterOnes {
        IterOnes { rest: self }
    }
}

/// [`SlotMask::iter_ones`].
#[allow(dead_code)]
pub(crate) struct IterOnes {
    rest: SlotMask,
}

impl Iterator for IterOnes {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        let mut k = 0;
        while k < SLOT_WORDS {
            let w = self.rest.0[k];
            if w != 0 {
                self.rest.0[k] = w & (w - 1);
                return Some(k * 64 + w.trailing_zeros() as usize);
            }
            k += 1;
        }
        None
    }
}

impl BitOr for SlotMask {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl BitOrAssign for SlotMask {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl BitAnd for SlotMask {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        self.intersection(rhs)
    }
}

impl BitAndAssign for SlotMask {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = self.intersection(rhs);
    }
}

impl Not for SlotMask {
    type Output = Self;
    #[inline]
    fn not(self) -> Self {
        self.complement()
    }
}

/// Bits in one atomic word: a machine word, so a target whose widest native
/// atomic is 32 bits uses 32-bit words.
const ATOMIC_WORD_BITS: usize = usize::BITS as usize;

// Issue 2541 -- the atomic words cover the image's OWN callback count, not
// the slot masks' capacity, rounded up to a machine word: an image of up to
// `usize::BITS` callbacks keeps exactly the one word it always had (32 slots
// on a 32-bit target, 64 on a 64-bit one), so its wake path is unchanged; a
// bigger image gets one more word per `usize::BITS` callbacks, up to the
// capacity. nros-node's own unit tests cover the whole capacity.
/// Atomic words in an [`AtomicSlotMask`].
#[allow(dead_code)]
pub(crate) const ATOMIC_WORDS: usize = {
    let cap_words = CALLBACK_SLOT_CAPACITY.div_ceil(ATOMIC_WORD_BITS);
    let need = if cfg!(test) {
        CALLBACK_SLOT_CAPACITY
    } else if crate::config::MAX_EXECUTOR_CBS < CALLBACK_SLOT_CAPACITY {
        crate::config::MAX_EXECUTOR_CBS
    } else {
        CALLBACK_SLOT_CAPACITY
    };
    let words = need.div_ceil(ATOMIC_WORD_BITS);
    let words = if words == 0 { 1 } else { words };
    if words < cap_words { words } else { cap_words }
};

/// One atomic word and one mask word: every [`AtomicSlotMask`] operation then
/// takes the single-word path the masks had before they grew words, so such
/// an image compiles to the code it always did.
const ONE_WORD: bool = ATOMIC_WORDS == 1 && SLOT_WORDS == 1;

/// Slots an [`AtomicSlotMask`] can name: the cross-thread wake words'
/// width. A slot past it keeps the session-scoped wake, as a slot past one
/// `usize` always did.
#[allow(dead_code)]
pub(crate) const ATOMIC_SLOTS: usize = ATOMIC_WORDS * ATOMIC_WORD_BITS;

const _: () = assert!(
    64 % ATOMIC_WORD_BITS == 0,
    "a mask word splits into whole atomic words"
);

/// A [`SlotMask`] other threads (and interrupts) write: one
/// `portable_atomic::AtomicUsize` per machine word. Every operation on it is
/// a sequence of single-word atomic operations, `SeqCst` as the one word
/// was; a multi-word read is therefore not one snapshot, which no reader
/// needs (each slot's bit is independent).
#[allow(dead_code)]
pub(crate) struct AtomicSlotMask([portable_atomic::AtomicUsize; ATOMIC_WORDS]);

impl Default for AtomicSlotMask {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for AtomicSlotMask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.load().fmt(f)
    }
}

#[allow(dead_code)] // each build uses the subset its features reach
impl AtomicSlotMask {
    pub(crate) const fn new() -> Self {
        Self([const { portable_atomic::AtomicUsize::new(0) }; ATOMIC_WORDS])
    }

    /// The atomic word holding `slot`, and the slot's bit in it.
    #[inline]
    const fn locate(slot: usize) -> (usize, usize) {
        (slot / ATOMIC_WORD_BITS, 1usize << (slot % ATOMIC_WORD_BITS))
    }

    /// The part of `mask` atomic word `j` covers.
    #[inline]
    const fn part(mask: SlotMask, j: usize) -> usize {
        let bit = j * ATOMIC_WORD_BITS;
        (mask.0[bit / 64] >> (bit % 64)) as usize
    }

    /// Fold atomic word `j`'s value `v` into `mask`.
    #[inline]
    fn fold(mask: &mut SlotMask, j: usize, v: usize) {
        let bit = j * ATOMIC_WORD_BITS;
        mask.0[bit / 64] |= (v as u64) << (bit % 64);
    }

    /// Set `slot`'s bit: one `fetch_or`. A slot past the capacity sets
    /// nothing.
    #[inline]
    pub(crate) fn set(&self, slot: usize) {
        if slot < ATOMIC_SLOTS {
            let (j, b) = Self::locate(slot);
            self.0[j].fetch_or(b, portable_atomic::Ordering::SeqCst);
        }
    }

    /// Whether `slot`'s bit is set.
    #[inline]
    pub(crate) fn test(&self, slot: usize) -> bool {
        if slot >= ATOMIC_SLOTS {
            return false;
        }
        let (j, b) = Self::locate(slot);
        self.0[j].load(portable_atomic::Ordering::SeqCst) & b != 0
    }

    /// Clear `slot`'s bit only if the word still reads `seen` (one
    /// `compare_exchange` on the slot's word). Returns whether it did.
    #[inline]
    pub(crate) fn clear_if_word_unchanged(&self, slot: usize, seen: &SlotMask) -> bool {
        if slot >= ATOMIC_SLOTS {
            return false;
        }
        let (j, b) = Self::locate(slot);
        let was = Self::part(*seen, j);
        self.0[j]
            .compare_exchange(
                was,
                was & !b,
                portable_atomic::Ordering::SeqCst,
                portable_atomic::Ordering::SeqCst,
            )
            .is_ok()
    }

    /// Every bit, word by word.
    #[inline]
    pub(crate) fn load(&self) -> SlotMask {
        if ONE_WORD {
            return SlotMask::from_word0(self.0[0].load(portable_atomic::Ordering::SeqCst) as u64);
        }
        let mut m = SlotMask::EMPTY;
        for (j, w) in self.0.iter().enumerate() {
            Self::fold(&mut m, j, w.load(portable_atomic::Ordering::SeqCst));
        }
        m
    }

    /// Replace every word.
    #[inline]
    pub(crate) fn store(&self, mask: SlotMask) {
        for (j, w) in self.0.iter().enumerate() {
            w.store(Self::part(mask, j), portable_atomic::Ordering::SeqCst);
        }
    }

    /// Take every bit: one `swap(0)` per word.
    #[inline]
    pub(crate) fn take(&self) -> SlotMask {
        if ONE_WORD {
            return SlotMask::from_word0(
                self.0[0].swap(0, portable_atomic::Ordering::SeqCst) as u64
            );
        }
        let mut m = SlotMask::EMPTY;
        for (j, w) in self.0.iter().enumerate() {
            Self::fold(&mut m, j, w.swap(0, portable_atomic::Ordering::SeqCst));
        }
        m
    }

    /// Take the bits of `mask` and leave the rest: one `fetch_and` per word
    /// `mask` touches (a `swap(0)` where it covers the whole word).
    #[inline]
    pub(crate) fn take_masked(&self, mask: SlotMask) -> SlotMask {
        if ONE_WORD {
            // The one-word code as it was before the masks grew words.
            let part = mask.word(0) as usize;
            let v = self.0[0].fetch_and(!part, portable_atomic::Ordering::SeqCst) & part;
            return SlotMask::from_word0(v as u64);
        }
        let mut m = SlotMask::EMPTY;
        for (j, w) in self.0.iter().enumerate() {
            let part = Self::part(mask, j);
            let v = match part {
                0 => continue,
                usize::MAX => w.swap(0, portable_atomic::Ordering::SeqCst),
                _ => w.fetch_and(!part, portable_atomic::Ordering::SeqCst) & part,
            };
            Self::fold(&mut m, j, v);
        }
        m
    }

    /// Whether any bit of `mask` is set.
    #[inline]
    pub(crate) fn intersects(&self, mask: SlotMask) -> bool {
        if ONE_WORD {
            return self.0[0].load(portable_atomic::Ordering::SeqCst) as u64 & mask.word(0) != 0;
        }
        self.0.iter().enumerate().any(|(j, w)| {
            let part = Self::part(mask, j);
            part != 0 && w.load(portable_atomic::Ordering::SeqCst) & part != 0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[test]
    fn the_unit_tests_run_more_than_one_word() {
        const { assert!(SLOT_WORDS >= 2, "cfg(test) floors the masks at two words") };
        assert_eq!(SlotMask::CAPACITY, 64 * SLOT_WORDS);
    }

    #[test]
    fn bits_on_each_side_of_a_word_boundary_are_distinct() {
        for i in [0usize, 1, 63, 64, 65, 127] {
            let m = SlotMask::bit(i);
            assert!(m.test(i), "slot {i}");
            assert_eq!(m.count(), 1);
            assert_eq!(m.first_set(), Some(i));
            for j in [0usize, 1, 63, 64, 65, 127] {
                assert_eq!(m.test(j), i == j, "bit {i} read at {j}");
            }
        }
        assert_ne!(SlotMask::bit(0), SlotMask::bit(64), "no aliasing mod 64");
        assert_ne!(SlotMask::bit(63), SlotMask::bit(127));
    }

    #[test]
    fn an_index_past_the_capacity_is_in_no_mask() {
        let cap = SlotMask::CAPACITY;
        assert!(SlotMask::bit(cap).is_empty());
        assert!(!SlotMask::ALL.test(cap));
        let mut m = SlotMask::EMPTY;
        m.set(cap);
        assert!(m.is_empty());
        m.set(cap - 1);
        assert_eq!(m.iter_ones().collect::<Vec<_>>(), [cap - 1]);
        m.clear(cap);
        assert_eq!(m.count(), 1);
    }

    #[test]
    fn set_algebra_spans_the_words() {
        let a = SlotMask::bit(3) | SlotMask::bit(64) | SlotMask::bit(127);
        let b = SlotMask::bit(64) | SlotMask::bit(100);
        assert_eq!((a & b), SlotMask::bit(64));
        assert_eq!((a | b).count(), 4);
        assert_eq!(a.and_not(b), SlotMask::bit(3) | SlotMask::bit(127));
        assert!(a.intersects(b));
        assert!(!a.intersects(SlotMask::bit(100) | SlotMask::bit(4)));
        assert!(a.contains_all(SlotMask::bit(127) | SlotMask::bit(3)));
        assert!(!a.contains_all(b));
        assert_eq!((!a).count(), SlotMask::CAPACITY as u32 - 3);
        assert!(!(!a).test(64));
        assert!((!a).test(63));
        assert_eq!(a.iter_ones().collect::<Vec<_>>(), [3, 64, 127]);
        let mut c = a;
        c.clear(64);
        assert_eq!(c.iter_ones().collect::<Vec<_>>(), [3, 127]);
        assert_eq!(SlotMask::EMPTY.first_set(), None);
        assert_eq!(SlotMask::ALL.count(), SlotMask::CAPACITY as u32);
        assert_eq!(
            SlotMask::from_word0(0b101).iter_ones().collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(SlotMask::bit(127).word(1), 1u64 << 63);
    }

    #[test]
    fn the_atomic_mask_takes_and_leaves_bits_across_words() {
        let a = AtomicSlotMask::new();
        for i in [0usize, 31, 32, 63, 64, 127] {
            a.set(i);
            assert!(a.test(i), "slot {i}");
        }
        a.set(SlotMask::CAPACITY);
        let all = [0usize, 31, 32, 63, 64, 127]
            .into_iter()
            .fold(SlotMask::EMPTY, |m, i| m.with(i));
        assert_eq!(a.load(), all);
        let band = SlotMask::bit(32) | SlotMask::bit(127) | SlotMask::bit(5);
        assert!(a.intersects(band));
        assert_eq!(a.take_masked(band), SlotMask::bit(32) | SlotMask::bit(127));
        assert_eq!(a.load(), all.and_not(band));
        assert_eq!(a.take_masked(SlotMask::ALL), all.and_not(band));
        assert!(a.load().is_empty());

        a.store(SlotMask::bit(64) | SlotMask::bit(1));
        let seen = a.load();
        a.set(65);
        assert!(!a.clear_if_word_unchanged(64, &seen), "word 64 moved");
        let seen = a.load();
        assert!(a.clear_if_word_unchanged(64, &seen));
        assert_eq!(a.take(), SlotMask::bit(65) | SlotMask::bit(1));
    }
}

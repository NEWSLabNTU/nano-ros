//! Caller-supplied storage for the component pool — phase-391 W5, step 2.
//!
//! Mirrors [`nros_node::ExecutorSizing`] one layer up, and
//! for the same stated reason: **public + non-generic**, the "C/C++ is a thin
//! wrapper" principle. The entry, the macro and the FFI seam supply these as
//! plain `usize`s rather than as const generics C cannot name —
//! `node_runtime` carries nine `extern "C"` sites and backs
//! `__nros_component_<pkg>_install`.
//!
//! This module is INERT on its own: it computes how large a backing must be.
//! The pool that carves one is W5 step 3; until then nothing calls
//! [`RuntimeSizing::u64_len`] except its tests.
//!
//! Ungated deliberately — the arithmetic is useful for sizing a `static`
//! whether or not the runtime module that consumes it is compiled in.

use crate::config::{COMPONENT_SLOT_BYTES, MAX_COMPONENTS};

/// Per-image component-pool sizing — how many components the runtime can hold
/// and how much erased storage each one's `TypedSlot<C>` gets.
///
/// `slot_bytes` is a BYTE budget rather than a type because the pool is
/// heterogeneous (`TypedSlot<C>` is generic over `C`) and the FFI seam cannot
/// name a generic. A component whose slot does not fit is a registration
/// error, not a compile error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeSizing {
    /// Component pool slots.
    pub components: usize,
    /// Bytes of erased slot storage per component.
    pub slot_bytes: usize,
}

impl RuntimeSizing {
    /// The build-time default, from the `NROS_RUNTIME_*` knobs.
    pub const DEFAULT: Self = Self {
        components: MAX_COMPONENTS,
        slot_bytes: COMPONENT_SLOT_BYTES,
    };

    /// `u64` words a backing must hold for this sizing.
    ///
    /// `u64` because the backing is 8-aligned, which covers every field the
    /// pool stores without hand-aligning — the same reason
    /// `executor_storage_u64_len` uses it.
    pub const fn u64_len(&self) -> usize {
        // Each slot is its erased storage rounded up to the 8-byte alignment
        // the backing already guarantees.
        let per_slot = self.slot_bytes.div_ceil(8);
        per_slot * self.components
    }
}

impl Default for RuntimeSizing {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A carved component-pool slot: `slot_bytes` of 8-aligned storage for one
/// type-erased `TypedSlot<C>`.
///
/// The slot outlives every closure that dispatches through it because the
/// BACKING is `'static` — which is the point of W5, and why the
/// `Arc<ComponentCell>` refcount that currently proves that lifetime becomes
/// unnecessary.
pub struct Slot<'s> {
    storage: &'s mut [core::mem::MaybeUninit<u8>],
}

impl Slot<'_> {
    /// Bytes available for the erased slot value.
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    /// Whether `T` fits this slot, in both size and alignment.
    ///
    /// A `false` here is a REGISTRATION error, not a compile error: the pool is
    /// heterogeneous and the FFI seam cannot name a generic. Callers surface it
    /// as "raise NROS_RUNTIME_COMPONENT_SLOT_BYTES".
    pub fn fits<T>(&self) -> bool {
        self.storage.len() >= core::mem::size_of::<T>()
            && (self.storage.as_ptr() as usize).is_multiple_of(core::mem::align_of::<T>())
    }

    /// Pointer to the slot's storage, for an in-place write of `TypedSlot<C>`.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.storage.as_mut_ptr() as *mut u8
    }
}

/// Carve `backing` into `sizing.components` slots of `sizing.slot_bytes`.
///
/// # Panics
///
/// If `backing` is too small, naming both sizes. Fail-loud on EVERY profile,
/// not `debug_assert!` — embedded release builds strip debug assertions, and a
/// short backing is silent memory corruption rather than a wrong answer. Same
/// reasoning as `executor::storage::carve` (issue #131, where a stale config
/// mirror surfaced as a `jalr -> 0`).
pub fn carve(
    backing: &mut [core::mem::MaybeUninit<u64>],
    sizing: RuntimeSizing,
) -> impl Iterator<Item = Slot<'_>> {
    let need = sizing.u64_len();
    assert!(
        backing.len() >= need,
        "component pool backing too small: {} u64 words < {} required for {} slot(s) \
         of {} bytes — size it with RuntimeSizing::u64_len()",
        backing.len(),
        need,
        sizing.components,
        sizing.slot_bytes,
    );
    let len_bytes = backing.len() * 8;
    // SAFETY: reinterpreting `MaybeUninit<u64>` as `MaybeUninit<u8>` reads no
    // value and only WIDENS the alignment guarantee; the length scales by 8.
    let bytes: &mut [core::mem::MaybeUninit<u8>] = unsafe {
        core::slice::from_raw_parts_mut(
            backing.as_mut_ptr() as *mut core::mem::MaybeUninit<u8>,
            len_bytes,
        )
    };
    let per_slot = (sizing.slot_bytes.div_ceil(8) * 8).max(1);
    bytes
        .chunks_exact_mut(per_slot)
        .take(sizing.components)
        .map(|storage| Slot { storage })
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;

    #[test]
    fn default_comes_from_the_knobs() {
        assert_eq!(RuntimeSizing::DEFAULT.components, MAX_COMPONENTS);
        assert_eq!(RuntimeSizing::DEFAULT.slot_bytes, COMPONENT_SLOT_BYTES);
    }

    #[test]
    fn u64_len_rounds_each_slot_up_not_the_total() {
        // 12 bytes is 1.5 words; each SLOT rounds to 2, so 3 slots need 6 —
        // not `(12 * 3) / 8 = 4.5 -> 5`. Rounding the total would under-size
        // every slot after the first.
        let s = RuntimeSizing {
            components: 3,
            slot_bytes: 12,
        };
        assert_eq!(s.u64_len(), 6);
    }

    #[test]
    fn zero_components_needs_no_backing() {
        let s = RuntimeSizing {
            components: 0,
            slot_bytes: 512,
        };
        assert_eq!(s.u64_len(), 0);
    }

    #[test]
    fn carve_yields_exactly_the_requested_slots() {
        let s = RuntimeSizing {
            components: 3,
            slot_bytes: 16,
        };
        let mut backing = [core::mem::MaybeUninit::<u64>::uninit(); 6];
        let slots: alloc::vec::Vec<_> = carve(&mut backing, s).collect();
        assert_eq!(slots.len(), 3, "one slot per component, no more");
        for sl in &slots {
            assert!(sl.capacity() >= 16, "each slot holds its byte budget");
        }
    }

    #[test]
    fn carve_slots_do_not_overlap() {
        let s = RuntimeSizing {
            components: 2,
            slot_bytes: 8,
        };
        let mut backing = [core::mem::MaybeUninit::<u64>::uninit(); 2];
        let mut slots: alloc::vec::Vec<_> = carve(&mut backing, s).collect();
        let a = slots[0].as_mut_ptr() as usize;
        let b = slots[1].as_mut_ptr() as usize;
        // Overlapping slots are the corruption this wave exists to avoid.
        assert!(b >= a + 8, "slot 1 starts at or after the end of slot 0");
    }

    #[test]
    #[should_panic(expected = "component pool backing too small")]
    fn carve_refuses_a_short_backing() {
        // Negative control: one word short must PANIC, not hand out slots that
        // run past the end. Verified to fail when the assert is removed.
        let s = RuntimeSizing {
            components: 4,
            slot_bytes: 64,
        };
        let mut backing = [core::mem::MaybeUninit::<u64>::uninit(); 31];
        let _ = carve(&mut backing, s).count();
    }

    #[test]
    fn a_type_too_large_for_its_slot_is_refused_not_truncated() {
        let s = RuntimeSizing {
            components: 1,
            slot_bytes: 8,
        };
        let mut backing = [core::mem::MaybeUninit::<u64>::uninit(); 1];
        let slot = carve(&mut backing, s).next().unwrap();
        assert!(slot.fits::<u64>(), "8 bytes fits an 8-byte slot");
        assert!(
            !slot.fits::<[u64; 4]>(),
            "32 bytes does not fit an 8-byte slot"
        );
    }

    #[test]
    fn the_default_sizing_is_not_accidentally_empty() {
        // Guards the case this campaign keeps hitting: a figure that passes
        // because nothing is in it.
        assert!(RuntimeSizing::DEFAULT.u64_len() > 0);
    }
}

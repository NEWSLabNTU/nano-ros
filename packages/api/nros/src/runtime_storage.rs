//! Caller-supplied storage for the component pool — phase-391 W5, step 2.
//!
//! Mirrors [`nros_node::executor::storage::ExecutorSizing`] one layer up, and
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

#[cfg(test)]
mod tests {
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
    fn the_default_sizing_is_not_accidentally_empty() {
        // Guards the case this campaign keeps hitting: a figure that passes
        // because nothing is in it.
        assert!(RuntimeSizing::DEFAULT.u64_len() > 0);
    }
}

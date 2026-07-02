//! phase-271 — per-entry [`Executor`](super::spin::Executor) storage (issue 0110).
//!
//! The executor's six sized arrays (callback table + arena + scheduling-context
//! tables) used to be inline fields sized by build-time consts baked into
//! `nros-node` — one size for every entry sharing a compiled `nros-node`. Here the
//! ENTRY supplies its own storage, sized to its topology, so a fat native entry
//! and a lean embedded entry in one workspace each get the right size with no
//! workspace-global env.
//!
//! Per the "C/C++ is a thin wrapper of Rust" principle the PUBLIC API stays
//! generic-free: the entry hands a raw, 8-aligned `&mut [MaybeUninit<u64>]` backing
//! (sized via [`executor_storage_u64_len`]); `nros-node` carves it privately into
//! the typed sub-slices ([`carve`]). The only `unsafe` is that carve, validated
//! against the `#[repr(C)]` reference [`ExecutorStorage`] layout by unit test.

use core::{
    alloc::Layout,
    mem::{MaybeUninit, align_of, size_of},
};

use super::{
    arena::CallbackMeta,
    sched_context::{SchedContext, SchedContextId, SporadicState},
};

#[cfg(feature = "alloc")]
type SporadicAtomic = (
    portable_atomic_util::Arc<super::sched_context::AtomicSporadicState>,
    super::spin::OpaqueTimerHandle,
);

/// The typed reference layout the [`carve`] mirrors. `#[repr(C)]` so its field
/// offsets are the deterministic declaration-order layout the const-fn below
/// reproduces; a unit test asserts they agree. Only referenced by tests.
#[cfg(test)]
#[repr(C)]
pub(crate) struct ExecutorStorage<const CBS: usize, const SC: usize, const ARENA: usize> {
    arena: [MaybeUninit<u8>; ARENA],
    entries: [Option<CallbackMeta>; CBS],
    sched_contexts: [Option<SchedContext>; SC],
    sched_context_bindings: [SchedContextId; CBS],
    sporadic_states: [Option<SporadicState>; SC],
    #[cfg(feature = "alloc")]
    sporadic_atomic_states: [Option<SporadicAtomic>; SC],
}

/// The typed, mutable sub-slices an [`Executor`](super::spin::Executor) borrows
/// from a carved backing. Element memory is initialised by [`carve`].
pub(crate) struct ExecutorSlices<'s> {
    pub(crate) arena: &'s mut [MaybeUninit<u8>],
    pub(crate) entries: &'s mut [Option<CallbackMeta>],
    pub(crate) sched_contexts: &'s mut [Option<SchedContext>],
    pub(crate) sched_context_bindings: &'s mut [SchedContextId],
    pub(crate) sporadic_states: &'s mut [Option<SporadicState>],
    #[cfg(feature = "alloc")]
    pub(crate) sporadic_atomic_states: &'s mut [Option<SporadicAtomic>],
}

/// Byte offsets of each field within the backing + total size/align. Computed
/// identically by [`executor_storage_layout`] and [`carve`] (single source of
/// truth), reproducing `#[repr(C)]` declaration-order layout.
struct FieldOffsets {
    arena: usize,
    entries: usize,
    sched_contexts: usize,
    sched_context_bindings: usize,
    sporadic_states: usize,
    #[cfg(feature = "alloc")]
    sporadic_atomic_states: usize,
    size: usize,
    align: usize,
}

const fn align_up(off: usize, align: usize) -> usize {
    off.div_ceil(align) * align
}

const fn compute_offsets(cbs: usize, sc: usize, arena: usize) -> FieldOffsets {
    let mut off = 0usize;
    let mut max_align = 1usize;

    // arena: [MaybeUninit<u8>; arena] — align 1, at offset 0.
    let arena_off = 0usize;
    off += arena;

    macro_rules! place {
        ($n:expr, $ty:ty) => {{
            let a = align_of::<$ty>();
            if a > max_align {
                max_align = a;
            }
            off = align_up(off, a);
            let at = off;
            off += $n * size_of::<$ty>();
            at
        }};
    }

    let entries = place!(cbs, Option<CallbackMeta>);
    let sched_contexts = place!(sc, Option<SchedContext>);
    let sched_context_bindings = place!(cbs, SchedContextId);
    let sporadic_states = place!(sc, Option<SporadicState>);
    #[cfg(feature = "alloc")]
    let sporadic_atomic_states = place!(sc, Option<SporadicAtomic>);

    let size = align_up(off, max_align);
    FieldOffsets {
        arena: arena_off,
        entries,
        sched_contexts,
        sched_context_bindings,
        sporadic_states,
        #[cfg(feature = "alloc")]
        sporadic_atomic_states,
        size,
        align: max_align,
    }
}

/// Byte [`Layout`] of the backing needed for a `(cbs, sc, arena)`-sized executor.
/// Public + non-generic so the macro / FFI can size a raw backing.
pub const fn executor_storage_layout(cbs: usize, sc: usize, arena: usize) -> Layout {
    let o = compute_offsets(cbs, sc, arena);
    // SAFETY: `align` is a power of two (a `max` of `align_of` results) and `size`
    // is rounded up to it; both are non-zero.
    unsafe { Layout::from_size_align_unchecked(o.size, o.align) }
}

/// Number of `u64` words a backing must hold for a `(cbs, sc, arena)`-sized
/// executor. `u64` backing is 8-aligned, which covers every field (all
/// `align_of ≤ 8`; asserted in tests), so the entry never hand-aligns. The macro
/// emits `[MaybeUninit<u64>; executor_storage_u64_len(N, SC, A)]`.
pub const fn executor_storage_u64_len(cbs: usize, sc: usize, arena: usize) -> usize {
    executor_storage_layout(cbs, sc, arena).size().div_ceil(8)
}

/// Per-entry executor sizing — the entity counts an [`Executor`](super::spin::Executor)
/// is built to hold. **Public + non-generic** (the "C/C++ is a thin wrapper"
/// principle): the entry / macro / FFI supplies these as plain `usize`s rather
/// than as type/const generics C can't name. Used to size + carve the backing.
///
/// `cbs` is capped at 64 by the executor's `u64` ready-set bitmask (asserted in
/// [`carve`]-time / `open_in`).
#[derive(Clone, Copy)]
pub struct ExecutorSizing {
    /// Callback-table slots (`entries` + per-entry SC bindings). ≤ 64.
    pub cbs: usize,
    /// Scheduling-context slots (`sched_contexts` + sporadic state tables).
    pub sc: usize,
    /// Bump-allocator arena size in bytes.
    pub arena: usize,
}

impl ExecutorSizing {
    /// The build-time default (`MAX_CBS`/`MAX_SC`/`ARENA_SIZE` consts) — the
    /// backward-compatible size the `alloc` convenience constructors leak.
    pub const DEFAULT: Self = Self {
        cbs: crate::config::MAX_CBS,
        sc: crate::config::MAX_SC,
        arena: crate::config::ARENA_SIZE,
    };

    /// `u64` words a backing must hold for this sizing (see
    /// [`executor_storage_u64_len`]).
    pub const fn u64_len(&self) -> usize {
        executor_storage_u64_len(self.cbs, self.sc, self.arena)
    }
}

/// The exact `#[repr(C)]` byte layout the C/C++ FFI's inline executor buffer must
/// hold: an [`Executor`](super::spin::Executor)`<'static>` header immediately
/// followed by a default-sized ([`ExecutorSizing::DEFAULT`]) storage backing.
///
/// The FFI keeps the executor inline (heap-free — matching the Rust no-alloc
/// requirement) and carves its per-entry tables from the SAME buffer's
/// [`backing`](Self::backing) tail. Because that buffer is **pinned** — the C
/// caller allocates it, it is initialised in place, and it is only ever reached
/// through a stable `nros_executor_t*` (never moved after init) — the resulting
/// self-borrow (the header's slices pointing into the same struct's tail) is
/// sound. The FFI probes `size_of` of THIS type (not bare `Executor`) to size
/// its `_opaque` array, and reinterprets `_opaque` as `*mut ExecutorInlineStorage`
/// (the executor stays at offset 0, so existing offset-0 accessors are unchanged).
#[repr(C)]
pub struct ExecutorInlineStorage {
    /// The executor, written in place (offset 0) by `from_session_ptr_in`.
    pub exec: MaybeUninit<super::spin::Executor<'static>>,
    /// The carved backing the executor's slices borrow (the buffer's tail).
    pub backing: [MaybeUninit<u64>; ExecutorSizing::DEFAULT.u64_len()],
}

/// Carve an 8-aligned `u64` backing into the typed, initialised executor slices.
///
/// # Safety
/// - `backing.len() * 8` must be ≥ `executor_storage_layout(cbs, sc, arena).size()`.
/// - The returned slices alias `backing`; it must outlive them (the `'s` bound)
///   and not be otherwise accessed while they live.
///
/// Element memory is initialised here (`entries`/SC tables → `None`, bindings →
/// `SchedContextId(0)`), so the returned `&mut [T]` reference validly-init memory.
pub(crate) unsafe fn carve<'s>(
    backing: &'s mut [MaybeUninit<u64>],
    cbs: usize,
    sc: usize,
    arena: usize,
) -> ExecutorSlices<'s> {
    let o = compute_offsets(cbs, sc, arena);
    debug_assert!(
        backing.len() * 8 >= o.size,
        "executor backing too small: {} bytes < {}",
        backing.len() * 8,
        o.size
    );
    let base = backing.as_mut_ptr() as *mut u8;

    unsafe {
        // arena — no init needed (MaybeUninit).
        let arena_s =
            core::slice::from_raw_parts_mut(base.add(o.arena) as *mut MaybeUninit<u8>, arena);

        let entries_p = base.add(o.entries) as *mut Option<CallbackMeta>;
        let mut i = 0;
        while i < cbs {
            entries_p.add(i).write(None);
            i += 1;
        }
        let entries_s = core::slice::from_raw_parts_mut(entries_p, cbs);

        let sc_p = base.add(o.sched_contexts) as *mut Option<SchedContext>;
        let mut i = 0;
        while i < sc {
            sc_p.add(i).write(None);
            i += 1;
        }
        let sched_contexts_s = core::slice::from_raw_parts_mut(sc_p, sc);

        let bind_p = base.add(o.sched_context_bindings) as *mut SchedContextId;
        let mut i = 0;
        while i < cbs {
            bind_p.add(i).write(SchedContextId(0));
            i += 1;
        }
        let bindings_s = core::slice::from_raw_parts_mut(bind_p, cbs);

        let sp_p = base.add(o.sporadic_states) as *mut Option<SporadicState>;
        let mut i = 0;
        while i < sc {
            sp_p.add(i).write(None);
            i += 1;
        }
        let sporadic_s = core::slice::from_raw_parts_mut(sp_p, sc);

        #[cfg(feature = "alloc")]
        let atomic_s = {
            let ap = base.add(o.sporadic_atomic_states) as *mut Option<SporadicAtomic>;
            let mut i = 0;
            while i < sc {
                ap.add(i).write(None);
                i += 1;
            }
            core::slice::from_raw_parts_mut(ap, sc)
        };

        ExecutorSlices {
            arena: arena_s,
            entries: entries_s,
            sched_contexts: sched_contexts_s,
            sched_context_bindings: bindings_s,
            sporadic_states: sporadic_s,
            #[cfg(feature = "alloc")]
            sporadic_atomic_states: atomic_s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CBS: usize = crate::config::MAX_CBS;
    const SC: usize = crate::config::MAX_SC;
    const ARENA: usize = crate::config::ARENA_SIZE;

    #[test]
    fn layout_matches_typed_repr_c() {
        // The manual const-fn layout must equal the compiler's `#[repr(C)]` layout
        // of the typed storage — proof the carve offsets are the real field offsets.
        let got = executor_storage_layout(CBS, SC, ARENA);
        let want = Layout::new::<ExecutorStorage<CBS, SC, ARENA>>();
        assert_eq!(got.size(), want.size(), "size");
        assert_eq!(got.align(), want.align(), "align");
    }

    #[test]
    fn u64_backing_covers_all_field_aligns() {
        assert!(align_of::<Option<CallbackMeta>>() <= 8);
        assert!(align_of::<Option<SchedContext>>() <= 8);
        assert!(align_of::<SchedContextId>() <= 8);
        assert!(align_of::<Option<SporadicState>>() <= 8);
        assert!(executor_storage_layout(CBS, SC, ARENA).align() <= 8);
    }

    #[test]
    fn carve_yields_right_lengths_and_inits() {
        // Heap-allocate: the default test config (MAX_CBS/MAX_SC/ARENA_SIZE from
        // build.rs) makes this backing array tens of KB, well past
        // `clippy::large_stack_arrays`'s threshold — and the size here is
        // incidental (mirrors production config), not the point under test, so
        // boxing is the right fix rather than an allow.
        let mut backing =
            alloc::vec![const { MaybeUninit::<u64>::uninit() }; executor_storage_u64_len(CBS, SC, ARENA)]
                .into_boxed_slice();
        let s = unsafe { carve(&mut backing, CBS, SC, ARENA) };
        assert_eq!(s.arena.len(), ARENA);
        assert_eq!(s.entries.len(), CBS);
        assert_eq!(s.sched_contexts.len(), SC);
        assert_eq!(s.sched_context_bindings.len(), CBS);
        assert_eq!(s.sporadic_states.len(), SC);
        assert!(s.entries.iter().all(|e| e.is_none()));
        assert!(s.sched_context_bindings.iter().all(|b| b.0 == 0));
    }
}

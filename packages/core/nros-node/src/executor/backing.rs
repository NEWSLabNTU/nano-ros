//! phase-392 W6 — the executor's storage as a NAMED STATIC (RFC-0002 § 4.4b).
//!
//! # What this fixes, and what it does not
//!
//! Phase 392 prices an image's static RAM by reading its symbol table
//! (`just mem-report`). Every later wave of that campaign is defined as a saving
//! against those numbers, so a consumer the symbol table cannot see is a
//! consumer the campaign cannot price — and the executor's storage was exactly
//! that on every Rust image.
//!
//! **Not because the arena is inline.** It stopped being an inline
//! `[MaybeUninit<u8>; ARENA_SIZE]` field in phase-271 (issue 0110), and
//! [`Executor`](super::spin::Executor) has held `arena: &'s mut
//! [MaybeUninit<u8>]` — a slice carved out of caller-supplied backing — ever
//! since. Four documents, a build-script comment and a runtime advisory string
//! went on asserting the inline shape for five phases; phase-403 caught the
//! claim and phase-392 W6 corrected the sites.
//!
//! **Where that backing lives is the caller's choice**, and before this module
//! the tree answered it three different ways:
//!
//! * **C** — all 34 in-tree `nros_executor_t` objects are file-scope `static`,
//!   so the backing is carved from `_opaque` in `.bss`. Visible.
//! * **C++** — `nros::Node::GlobalStorageHolder<0>::storage`, a template static
//!   member, so also `.bss`. Visible.
//! * **Rust** — every board entry (`linux`, `zephyr`, `freertos`, `nuttx`,
//!   `threadx`, `esp32-qemu`, `mps2-an385`) reaches an `alloc` convenience
//!   constructor, which leaked a `Box`. **Invisible**: a heap allocation has no
//!   symbol. Measured on the native zenoh talker before this module: the largest
//!   `nros_node` RAM symbol in the whole image was **1 byte**.
//!
//! This module closes the Rust arm — one change, every Rust board.
//!
//! # It is a MOVE, not a saving, and the second half is the caller's
//!
//! The same bytes are reserved either way; what changes is *which budget pays*.
//! A leaked `Box` is drawn from the image's allocator arena. On a hosted target
//! that arena is the OS heap and has no fixed reservation, so the static costs
//! nothing that was not already spent. **On an RTOS it is itself a fixed
//! static** — `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` on Zephyr,
//! `configTOTAL_HEAP_SIZE` on FreeRTOS — already sized to hold this backing, so
//! turning the reservation on there without lowering that knob reserves the same
//! bytes twice.
//!
//! That pairing is per-image and only the image's author can measure it, so the
//! opt-out is a knob rather than a guess: `NROS_EXECUTOR_BACKING_U64S=0` emits
//! no static at all and restores the leak. A non-zero value overrides the
//! reservation's SIZE, which is also how a fat entry — one declaring
//! `max_callbacks` above `NROS_EXECUTOR_MAX_CBS` — keeps a static instead of
//! falling back to the heap.
//!
//! **The Zephyr `CONFIG_` spelling IS wired** (issue 1171; this paragraph said
//! the opposite until then). `zephyr/Kconfig` declares
//! `NROS_EXECUTOR_BACKING_U64S` with the tree's `-1` DERIVE sentinel as its
//! default, and `nros-node`'s build script reaches it through the same env →
//! `$DOTCONFIG` reader every other executor knob uses (issue 0460). `-1` does
//! not parse as a `usize`, which is exactly how every other `-1 = derive` knob
//! in this tree falls through to its crate default, so the shipped default is
//! still the derived size and no Zephyr image moved the day it landed — a
//! plain `default 0` would have meant "no static" on all of them, which is the
//! opposite of the default this chose.
//!
//! # Why an image would STATE the size rather than derive it
//!
//! Because the derivation cannot be paired with anything. Issue 1145 lowered
//! one leaf's `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` by a size copied out of
//! `nm` output, and that subtrahend is a function of the executor knobs: move
//! `MAX_CBS`, the arena or the rx buffer and it is silently stale in whichever
//! direction, with nothing to say so (issue 1171). It was also wrong on the
//! second board the same conf builds for — the derived size is **87,256 B on
//! `mps2_an385` and 88,328 B on `native_sim/native/64`**, because the carved
//! tables hold pointers, so one number cannot pair both.
//!
//! Stating it inverts the dependency: the reservation is `8 * words` bytes on
//! every target, the arena's lowering is arithmetic a gate can check
//! (`check-executor-backing-arena-pairing`), and a stated size BELOW what the
//! executor needs is the `const` assertion below — a compile error naming this
//! knob, which is the direction that would otherwise re-create the double
//! reservation in silence.
//!
//! # Why there is no `// nros-pool:` annotation
//!
//! `scripts/gen-pool-inventory.py` evaluates a pool as a PRODUCT of knobs at
//! their literal defaults. This one is a SUM — the executor's carved tables plus
//! the arena, laid out with alignment padding between them — and its largest
//! term, `ARENA_SIZE`, is itself derived
//! (`env_usize("NROS_EXECUTOR_ARENA_SIZE", derived_arena)` in
//! `nros-node/build.rs`), so the inventory would record it as a computed default
//! even if the sum were expressible. A formula that is right for one build and
//! wrong for the rest is the drift class this tree gates against, so this
//! follows the two documented deliberate non-annotations
//! (`nros_rmw_zenoh::shim::publisher`, `nros_rmw_cffi`): **the size is known to
//! the compiler, so read it from the compiler's output.** `mem-report` prices
//! the symbol from the ELF, exactly, with no formula to drift.
//!
//! # Placement
//!
//! `NROS_EXECUTOR_BACKING_SECTION` (build-time env, read by `nros-node`'s build
//! script) puts the static in a named section, for the amendment-A case where a
//! part has tightly-coupled memory the linker script can target — issue 0880
//! does the same for `nros_thread_stacks` with `DTCM`. Two constraints, both
//! load-bearing: **the section must be `NOLOAD`**, because this static is
//! uninitialised and a loadable section would add its whole size to the image's
//! flash footprint; and it must be reachable by every bus master that touches
//! the executor's buffers, which on Cortex-M7 the TCMs typically are not.

use core::mem::MaybeUninit;

#[cfg(nros_executor_backing_static)]
use portable_atomic::{AtomicBool, Ordering};

use super::storage::ExecutorSizing;

/// The reservation's size when nothing overrides it: one default-sized
/// executor's worth.
///
/// One executor, not a multiple, because an entry opens ONE through this road:
/// a tiered boot's SPAWNED tiers take their slots from the entry's own
/// [`TierExecutorBacking`] instead (issue 1571), each slot this many words.
///
/// Named by the GENERATED file when nothing overrides the size, so it is unused
/// under `NROS_EXECUTOR_BACKING_U64S=0` (no static) and under an explicit
/// override (a literal). Kept rather than `cfg`'d: it is what the override is
/// judged against, and a constant that disappears with its consumer cannot be
/// compared to anything.
#[allow(dead_code)]
pub const EXECUTOR_BACKING_DEFAULT_U64S: usize = ExecutorSizing::DEFAULT.u64_len();

// `EXECUTOR_BACKING`, its size const, and the optional
// `#[unsafe(link_section = …)]` on it are emitted by `build.rs`: `link_section`
// takes a string LITERAL and the section name is a build-time input, and the
// size may be overridden or the whole item suppressed. The file is EMPTY when
// `NROS_EXECUTOR_BACKING_U64S=0`, which is why the include is unconditional
// while everything that reads it is `cfg`-gated.
include!(concat!(env!("OUT_DIR"), "/nros_executor_backing.rs"));

/// The reservation must cover the sizing every `alloc` convenience constructor
/// passes, or it is dead weight every image carries and no executor ever uses.
///
/// A `const` assertion rather than a test, deliberately: the only way to make it
/// false is to set `NROS_EXECUTOR_BACKING_U64S` too low, and the person doing
/// that wants to hear about it from the build they just ran, not from a test
/// suite they may not run at all. `0` is the documented opt-out and is not
/// reached here — it emits no static, so this whole arm is `cfg`'d away.
///
/// **It also goes false the OTHER way, and a const assertion alone cannot see
/// that** (issue 1284): a conf that STATES the knob stays put while the executor
/// grows under it. That fails only in the image's own build, which no
/// merge-gating lane runs, so twelve Zephyr leaves drifted twice in a week. `just
/// check node-std-tests` now runs this assertion per stated claim at each
/// claimed board's pointer width, and `tests/executor_backing_claims.rs` checks
/// the host-width claims against [`EXECUTOR_BACKING_DEFAULT_U64S`] by number;
/// `check-executor-backing-arena-pairing --claims` says which confs claim what.
#[cfg(nros_executor_backing_static)]
const _: () = assert!(
    EXECUTOR_BACKING_U64S >= EXECUTOR_BACKING_DEFAULT_U64S,
    "NROS_EXECUTOR_BACKING_U64S is below the default executor sizing, so the \
     reservation can never be taken and is pure dead weight; use 0 to decline \
     the static entirely"
);

/// Has [`EXECUTOR_BACKING`] been handed out?
///
/// A latch, not a free list: the backing is handed out as `&'static mut` and
/// never returned, exactly like the `Box::leak` it replaces. An executor that
/// drops does not give it back — reclaiming it would need the executor's own
/// tables to be provably dead first, which `Drop` cannot show for a slice it has
/// already lent to `Dispatcher`.
#[cfg(nros_executor_backing_static)]
static TAKEN: AtomicBool = AtomicBool::new(false);

/// Hand out [`EXECUTOR_BACKING`], once, if it can hold `words`.
///
/// `None` means "use the heap", and every path to it is legitimate rather than
/// degraded — it is the caller's behaviour from before this module existed:
///
/// * the reservation is switched off (`NROS_EXECUTOR_BACKING_U64S=0`);
/// * another executor already took it (a second `Executor::open` in one
///   process — a tiered boot's spawned tiers no longer come here, they take
///   the entry's [`TierExecutorBacking`], issue 1571);
/// * this executor is sized past the reservation (a fat entry).
///
/// The `swap` is what makes the returned `&'static mut` sound: exactly one
/// caller observes `false`, so exactly one reference to the static ever exists.
#[cfg(nros_executor_backing_static)]
pub(crate) fn take(words: usize) -> Option<&'static mut [MaybeUninit<u64>]> {
    if words > EXECUTOR_BACKING_U64S {
        return None;
    }
    if TAKEN.swap(true, Ordering::AcqRel) {
        return None;
    }
    // SAFETY: the swap above succeeds exactly once for the life of the process,
    // so this is the only reference ever created to `EXECUTOR_BACKING`, and it
    // is `'static` because the static is. `words <= EXECUTOR_BACKING_U64S` was
    // checked, so the slice is in bounds.
    // The raw ref is bound first rather than dereferenced in place: `&mut
    // *(&raw mut X)` reads as `deref_addrof` to clippy, and the obvious
    // rewrite it suggests (`&mut X`) is the `static_mut_refs` hazard this
    // spelling exists to avoid.
    let ptr = &raw mut EXECUTOR_BACKING;
    let all: &'static mut [MaybeUninit<u64>; EXECUTOR_BACKING_U64S] = unsafe { &mut *ptr };
    Some(&mut all[..words])
}

/// The reservation is switched off, so there is nothing to hand out.
///
/// A stub rather than a `cfg` at the call site: `default_backing` reads the same
/// either way, which is the paired-stub idiom `trace_register` uses one file
/// over, and it keeps the `#[cfg]` in exactly one place.
#[cfg(not(nros_executor_backing_static))]
pub(crate) fn take(_words: usize) -> Option<&'static mut [MaybeUninit<u64>]> {
    None
}

/// Has the static been claimed? Tests only.
#[cfg(all(test, nros_executor_backing_static))]
pub(crate) fn is_taken() -> bool {
    TAKEN.load(Ordering::Acquire)
}

// ============================================================================
// issue 1571 — the SPAWNED tiers' executor backing, as the entry's own static
// ============================================================================
//
// A tiered boot opens one executor per tier. The boot tier takes
// `EXECUTOR_BACKING` through `Executor::open` like any single-executor entry;
// every OTHER tier used to reach `default_backing` too, find the latch already
// taken, and `Box::leak` a default-sized block — correctly sized, but invisible
// to `mem-report` and drawn from the allocator arena, where issue 1568 had just
// moved every C and C++ RTOS executor (tier and single) OUT of it.
//
// The fix is the C/C++ METHOD, not a second one: the tier count is known when
// the entry is generated, so the entry (`nros::main!`) emits ONE named static of
// `N - 1` slots — the C pack's `__nros_tier_executor_storage` — and the board's
// `run_tiers` hands each spawned tier its slot. The slot is typed, so its size is
// the executor's exact need by construction: the tier opens with
// `ExecutorSizing::DEFAULT` (see `Executor::open_with_session_slot`) and the slot
// is `ExecutorSizing::DEFAULT.u64_len()` words — the same const the boot
// reservation above is judged against, so there is ONE spelling of "a default
// executor's worth" for both.
//
// It is a MOVE, exactly like the boot reservation: on an RTOS the same bytes
// leave the allocator arena (`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`,
// `configTOTAL_HEAP_SIZE`, the NuttX kernel heap, the ThreadX byte pool) and
// become linker-visible `.bss`.

/// issue 1571 — one spawned tier executor's backing: exactly the words an
/// executor opened with [`ExecutorSizing::DEFAULT`] carves.
pub type TierExecutorBackingSlot = [MaybeUninit<u64>; EXECUTOR_BACKING_DEFAULT_U64S];

/// issue 1571 — the spawned tiers' executor backing, `N` slots in ONE named
/// static that the ENTRY owns.
///
/// `nros::main!` emits one per tiered entry, with `N` = the tier count minus the
/// boot tier (which takes the `EXECUTOR_BACKING` road), and passes
/// [`take`](Self::take) to the board's `run_tiers`. The board then refuses a
/// short block with [`check_tier_executor_backing`] — the Rust twin of the C
/// library's `nros_cpp_executor_storage_check`.
///
/// Placement is the caller's (RFC-0002 § 4.4b): the static lives in the entry's
/// `.bss`, so `mem-report` prices it by name.
pub struct TierExecutorBacking<const N: usize> {
    /// The same once-only latch as the boot reservation's, per static: the
    /// slots go out as `&'static mut`, so exactly one caller may observe them.
    taken: portable_atomic::AtomicBool,
    slots: core::cell::UnsafeCell<[TierExecutorBackingSlot; N]>,
}

// SAFETY: the slots are only reachable through `take`, whose latch hands them
// out at most once, so no two threads can ever hold a reference to them.
unsafe impl<const N: usize> Sync for TierExecutorBacking<N> {}

impl<const N: usize> TierExecutorBacking<N> {
    /// An untouched reservation. `const` so it can initialise a `static`, and
    /// uninitialised (plus a `false` latch), so the static is all-zero `.bss`
    /// and costs no flash.
    ///
    /// `large_stack_arrays` is allowed because this is only ever evaluated in
    /// CONST context (a `static` initialiser): the array is built by the
    /// compiler into `.bss`, never on a stack.
    #[allow(clippy::new_without_default, clippy::large_stack_arrays)]
    pub const fn new() -> Self {
        Self {
            taken: portable_atomic::AtomicBool::new(false),
            slots: core::cell::UnsafeCell::new(
                [[MaybeUninit::uninit(); EXECUTOR_BACKING_DEFAULT_U64S]; N],
            ),
        }
    }

    /// Hand out every slot, once. A second call returns an EMPTY slice, which
    /// the board's [`check_tier_executor_backing`] then refuses by name rather
    /// than aliasing a slot another executor is using.
    ///
    /// `mut_from_ref` is the point, not an accident: `&'static self` is how a
    /// `static` is reached, and the latch is what makes the one `&mut` sound —
    /// the same shape as the boot reservation's `take`.
    #[allow(clippy::mut_from_ref)]
    pub fn take(&'static self) -> &'static mut [TierExecutorBackingSlot] {
        if self.taken.swap(true, portable_atomic::Ordering::AcqRel) {
            return &mut [];
        }
        // SAFETY: the swap above succeeds exactly once per static, so this is
        // the only reference ever created to `slots`; `'static` because `self`
        // is.
        unsafe { &mut *self.slots.get() }
    }
}

/// issue 1571 — the tier backing a board was handed does not cover the tiers it
/// must spawn. Carries both counts so the refusal names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierBackingShort {
    /// Tiers the board spawns (every tier but the boot one).
    pub spawned: usize,
    /// Slots it was handed.
    pub slots: usize,
}

impl core::fmt::Display for TierBackingShort {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "tier executor backing holds {} slot(s) but {} spawned tier(s) need one each \
             — the entry's `TierExecutorBacking` was sized for a different tier table, \
             or taken twice (issue 1571)",
            self.slots, self.spawned
        )
    }
}

/// issue 1571 — the ONE refusal every board's `run_tiers` makes before it opens
/// anything: `n_tiers` tiers run, one on the boot executor, so `n_tiers - 1`
/// must each find a slot. Checked up front rather than at each spawn, so a
/// short block never leaves a half-spawned tier chain.
pub fn check_tier_executor_backing(
    n_tiers: usize,
    backing: &[TierExecutorBackingSlot],
) -> Result<(), TierBackingShort> {
    let spawned = n_tiers.saturating_sub(1);
    if backing.len() < spawned {
        return Err(TierBackingShort {
            spawned,
            slots: backing.len(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// issue 1598 — each spawned tier's TASK memory (stack + control block), the
// same "entry declares, board uses" method one step further: the executor
// backing above is in `.bss`, and so now is the task that runs it.
// ---------------------------------------------------------------------------

/// issue 1598 — the control-block region every [`TierTaskMemory`] carries,
/// in 8-byte words. An UPPER BOUND over the kernels that take it (a FreeRTOS
/// `StaticTask_t` on Cortex-M is about a hundred bytes); the port REFUSES a
/// region smaller than its own control block at spawn, by name, so a kernel
/// whose block outgrows this fails loudly rather than overrunning the stack
/// that follows. Rust cannot name a C type's size at macro time, which is why
/// this one number is a bound and the stack beside it is exact.
pub const TIER_TASK_TCB_U64S: usize = 64;

/// issue 1598 — one spawned tier's task memory as a board receives it: raw,
/// `Copy`, no lifetime. The ENTRY owns the bytes ([`TierTaskMemory`]); the
/// handout is once-only ([`TierTaskMemorySet::take`]), which is what makes the
/// raw pointers exclusive.
#[derive(Clone, Copy, Debug)]
pub struct TierTaskMemoryRaw {
    /// The stack: `stack_bytes` bytes, 64-byte aligned.
    pub stack: *mut u8,
    /// The stack's size in bytes — the tier's declared `stack_bytes` (or the
    /// family default), rounded up to whole words.
    pub stack_bytes: usize,
    /// The control-block region: `tcb_bytes` bytes, 64-byte aligned.
    pub tcb: *mut u8,
    /// [`TIER_TASK_TCB_U64S`] words.
    pub tcb_bytes: usize,
}

impl TierTaskMemoryRaw {
    /// "This tier has no task memory from the entry": the port keeps its own
    /// default (Zephyr's pool slot for a tier that declared no `stack_bytes`).
    pub const NONE: Self = Self {
        stack: core::ptr::null_mut(),
        stack_bytes: 0,
        tcb: core::ptr::null_mut(),
        tcb_bytes: 0,
    };
}

// SAFETY: the pointers name `'static` storage handed out once (see the set's
// latch), so moving the row to the task that uses it is the whole point.
unsafe impl Send for TierTaskMemoryRaw {}
unsafe impl Sync for TierTaskMemoryRaw {}

/// issue 1598 — one spawned tier's stack and control block, as ONE named static
/// the entry owns. `STACK_U64S` is the tier's stack in 8-byte words;
/// `TCB_U64S` the control-block region, [`TIER_TASK_TCB_U64S`] by default and
/// `0` for a kernel whose control block the port keeps itself (Zephyr's
/// `struct k_thread`, issue 1232 — a Rust entry cannot size it).
///
/// The control block comes first and the struct is 64-byte aligned, so the
/// stack starts on a boundary every in-tree kernel accepts for a thread stack
/// (the TCB region is a whole number of 64-byte lines).
#[repr(C, align(64))]
pub struct TierTaskMemory<const STACK_U64S: usize, const TCB_U64S: usize = TIER_TASK_TCB_U64S> {
    tcb: core::cell::UnsafeCell<[MaybeUninit<u64>; TCB_U64S]>,
    stack: core::cell::UnsafeCell<[MaybeUninit<u64>; STACK_U64S]>,
}

// SAFETY: the bytes are reached only through `raw`, collected into a
// `TierTaskMemorySet` whose latch hands them out at most once.
unsafe impl<const STACK_U64S: usize, const TCB_U64S: usize> Sync
    for TierTaskMemory<STACK_U64S, TCB_U64S>
{
}

impl<const STACK_U64S: usize, const TCB_U64S: usize> TierTaskMemory<STACK_U64S, TCB_U64S> {
    /// An untouched reservation, `const` so it initialises a `static`
    /// (all-zero `.bss`, no flash).
    #[allow(clippy::new_without_default, clippy::large_stack_arrays)]
    pub const fn new() -> Self {
        Self {
            tcb: core::cell::UnsafeCell::new([MaybeUninit::uninit(); TCB_U64S]),
            stack: core::cell::UnsafeCell::new([MaybeUninit::uninit(); STACK_U64S]),
        }
    }

    /// The raw row for this static — `const`, so the entry builds its
    /// [`TierTaskMemorySet`] in a `static` initialiser.
    pub const fn raw(&'static self) -> TierTaskMemoryRaw {
        TierTaskMemoryRaw {
            stack: self.stack.get() as *mut u8,
            stack_bytes: STACK_U64S * core::mem::size_of::<u64>(),
            tcb: self.tcb.get() as *mut u8,
            tcb_bytes: TCB_U64S * core::mem::size_of::<u64>(),
        }
    }
}

/// issue 1598 — the spawned tiers' task memory, `N` rows in spawn (chain)
/// order, behind a once-only latch.
pub struct TierTaskMemorySet<const N: usize> {
    taken: portable_atomic::AtomicBool,
    rows: [TierTaskMemoryRaw; N],
}

impl<const N: usize> TierTaskMemorySet<N> {
    /// `const`, for a `static` initialiser over the per-tier statics.
    pub const fn new(rows: [TierTaskMemoryRaw; N]) -> Self {
        Self {
            taken: portable_atomic::AtomicBool::new(false),
            rows,
        }
    }

    /// Hand out every row, once. A second call returns an EMPTY slice, which a
    /// board refuses by count rather than giving two tasks one stack.
    pub fn take(&'static self) -> &'static [TierTaskMemoryRaw] {
        if self.taken.swap(true, portable_atomic::Ordering::AcqRel) {
            return &[];
        }
        &self.rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The POSITIVE CONTROL, and the reason it and the two `None` cases share
    /// one test: `TAKEN` is process-scoped by design, so a second test would
    /// observe an already-consumed latch and assert nothing — the vacuous shape
    /// `check-no-vacuous-tests` exists to catch.
    ///
    /// A test that only ever saw `take` return `None` would pass against a `take`
    /// that can never succeed, which is exactly the bug that would make this
    /// whole module inert while reading as if it worked.
    ///
    /// `#[ignore]` because it needs a VIRGIN PROCESS, and that is a property of
    /// the subject rather than a wish: `TAKEN` is process-global, and
    /// `spin.rs`'s `default_backing` — reached by every `Executor::open` under
    /// the `alloc` feature — claims it. Any of this binary's other 365 tests
    /// that opens an executor therefore consumes the one handout first, and
    /// this test's own positive control (`take` returning `Some`) becomes
    /// impossible. Measured: passes alone, fails in the full lane at the
    /// `!is_taken()` assertion below.
    ///
    /// `just check node-std-tests` runs it in its own cargo invocation, which
    /// is a second process. Same shape and same answer as `boot_report::tests`
    /// one recipe over, and for the same underlying reason.
    #[cfg(nros_executor_backing_static)]
    #[test]
    #[ignore = "needs a virgin process: TAKEN is a process-global latch that \
                Executor::open consumes — run via `just check node-std-tests`"]
    fn the_static_is_handed_out_once_and_only_once() {
        // Too large for the reservation: refused BEFORE the latch is touched, so
        // this case cannot consume the one handout.
        assert!(
            take(EXECUTOR_BACKING_U64S + 1).is_none(),
            "a request past the reservation must fall back to the heap"
        );
        assert!(
            !is_taken(),
            "an over-large request must not claim the latch"
        );

        let first = take(EXECUTOR_BACKING_U64S).expect("the first taker gets the static");
        assert_eq!(
            first.len(),
            EXECUTOR_BACKING_U64S,
            "the handout is the requested length, not the whole reservation"
        );
        assert!(is_taken());

        assert!(
            take(1).is_none(),
            "a second taker must fall back to the heap — two `&'static mut` to \
             one static is UB, which is the whole reason for the latch"
        );
    }

    /// With the reservation off there is no static, so `take` is inert and no
    /// image carries the bytes.
    #[cfg(not(nros_executor_backing_static))]
    #[test]
    fn the_opt_out_removes_the_reservation_entirely() {
        assert!(
            take(1).is_none(),
            "NROS_EXECUTOR_BACKING_U64S=0 must hand out nothing"
        );
    }

    /// issue 1571 — the entry's tier backing goes out ONCE, whole, and each
    /// slot is exactly one default executor's worth. Its own static, so unlike
    /// the boot reservation's test it needs no virgin process.
    #[test]
    fn tier_backing_is_handed_out_once_and_each_slot_is_one_default_executor() {
        static TIERS: TierExecutorBacking<3> = TierExecutorBacking::new();
        let slots = TIERS.take();
        assert_eq!(slots.len(), 3, "the first taker gets every slot");
        assert_eq!(
            core::mem::size_of::<TierExecutorBackingSlot>(),
            8 * ExecutorSizing::DEFAULT.u64_len(),
            "a slot is exactly what a DEFAULT-sized executor carves"
        );
        assert!(
            TIERS.take().is_empty(),
            "a second taker must get nothing — two `&'static mut` to one slot is UB"
        );
    }

    /// issue 1598 — a tier's task memory is exactly the declared stack plus the
    /// stated control-block bound, aligned for a thread stack, and handed out
    /// ONCE: a second take is empty, never a second task on the same stack.
    #[test]
    fn tier_task_memory_is_the_declared_size_and_handed_out_once() {
        static A: TierTaskMemory<128> = TierTaskMemory::new();
        static B: TierTaskMemory<32> = TierTaskMemory::new();
        static SET: TierTaskMemorySet<2> = TierTaskMemorySet::new([A.raw(), B.raw()]);
        let rows = SET.take();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].stack_bytes, 128 * 8);
        assert_eq!(rows[1].stack_bytes, 32 * 8);
        assert_eq!(rows[0].tcb_bytes, TIER_TASK_TCB_U64S * 8);
        assert_eq!(rows[0].stack as usize % 64, 0, "stack is 64-byte aligned");
        assert_ne!(rows[0].stack, rows[1].stack);
        assert_eq!(
            core::mem::size_of::<TierTaskMemory<128>>(),
            (TIER_TASK_TCB_U64S + 128) * 8,
            "nothing but the TCB bound and the stack"
        );
        assert!(
            SET.take().is_empty(),
            "a second take must not alias the rows"
        );
    }

    /// issue 1571 — the one refusal: every tier but the boot one needs a slot.
    #[test]
    fn tier_backing_refuses_a_block_short_of_the_spawned_tiers() {
        static TWO: TierExecutorBacking<2> = TierExecutorBacking::new();
        let two = TWO.take();
        assert_eq!(check_tier_executor_backing(3, two), Ok(()));
        assert_eq!(
            check_tier_executor_backing(1, &[]),
            Ok(()),
            "boot tier only"
        );
        assert_eq!(
            check_tier_executor_backing(4, two),
            Err(TierBackingShort {
                spawned: 3,
                slots: 2
            })
        );
        assert_eq!(
            check_tier_executor_backing(2, &[]),
            Err(TierBackingShort {
                spawned: 1,
                slots: 0
            }),
            "a taken-twice (empty) block is refused, not aliased"
        );
    }
}

//! phase-417 W4.d — bounded storage so `get_or_create_logger` can hand out a
//! logger for a name nobody registered at compile time.
//!
//! ## The gap this closes
//!
//! [`crate::get_logger`] is a LOOKUP: it answers with
//! [`crate::DEFAULT_LOGGER`] for any name no `'static` [`crate::Logger`] was
//! [`crate::register_logger`]ed under. That is correct for the Rust call sites
//! it was written for, which declare their logger as a `static`. It is the
//! wrong answer for a wrapper: `rclcpp::get_logger("x")` and
//! `rcutils_get_logger("x")` CREATE, so a C caller doing
//!
//! ```c
//! nros_logger_set_level(nros_log_get_logger("nav"), NROS_LOG_SEVERITY_DEBUG);
//! ```
//!
//! over a lookup-only implementation would silently have moved the threshold
//! of the catch-all logger every other unregistered name also resolves to —
//! compile, and differ. RFC-0089 forbids exactly that, so the creation has to
//! be real, and by RFC-0019 it has to be real HERE rather than in `nros-c`.
//!
//! ## Bounded, because `no_std`
//!
//! Two static arenas, both sized by the `NROS_LOG_DYNAMIC_LOGGERS` knob
//! (phase-479 W5, RFC-0102 D5 — see `build.rs` for its ladder):
//! one for the [`crate::Logger`] values, one for their names (a `&'static str`
//! is what `Logger` holds, and a `const char *` from C is not one). Exhaustion
//! is REPORTED, never silently aliased onto the default logger — see
//! [`crate::get_or_create_logger`].

use core::{cell::UnsafeCell, mem::MaybeUninit};

use portable_atomic::{AtomicUsize, Ordering};

use crate::Logger;

/// How many loggers [`crate::get_or_create_logger`] may create.
///
/// The `NROS_LOG_DYNAMIC_LOGGERS` knob, resolved once by `build.rs` on the
/// RFC-0049 ladder: the environment (which is where an image's
/// `[image.<id>] env` lands) > Kconfig `CONFIG_NROS_LOG_DYNAMIC_LOGGERS` /
/// the board's `[board.knobs.log] dynamic_loggers` > 16. Linux host boards
/// state 32; MCU boards keep 16. `0` declines the arena entirely (creation
/// always fails, lookup still works), which is the right build for an image
/// whose loggers are all `static`.
///
/// The `dynamic-loggers-<N>` features are deprecated (one release): honoured
/// with a warning when no knob is stated, and a build error when they disagree
/// with one.
#[must_use]
pub const fn dynamic_logger_capacity() -> usize {
    DYNAMIC_LOGGER_CAPACITY
}

// `DYNAMIC_LOGGER_CAPACITY`, written by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/nros_log_config.rs"));

/// Name-arena bytes reserved per runtime logger.
///
/// An AVERAGE, not a per-name cap ([`MAX_LOGGER_NAME_LEN`] is that): the
/// arena is one bump region of `capacity * 24` bytes shared by every name.
const NAME_ARENA_BYTES_PER_LOGGER: usize = 24;

/// Longest logger name the arena will accept, in bytes.
///
/// A cap per NAME as well as in total, so one absurd name cannot consume the
/// budget every other logger in the image needs.
pub const MAX_LOGGER_NAME_LEN: usize = 48;

const CAPACITY: usize = dynamic_logger_capacity();
const NAME_ARENA_BYTES: usize = CAPACITY * NAME_ARENA_BYTES_PER_LOGGER;

struct Pool {
    loggers: UnsafeCell<[MaybeUninit<Logger>; CAPACITY]>,
    loggers_used: AtomicUsize,
    names_used: AtomicUsize,
}

/// The name arena, a static of its OWN rather than a field of [`POOL`].
///
/// phase-479 W5 — so its symbol is exactly `capacity * 24` bytes, with no
/// struct padding and no `Logger` (whose size moves with its fields) beside
/// it. `just mem-report` reads the image's capacity back out of that size
/// (`nros_log::pool::NAMES`), which is the only way a STATIC report can state
/// a count rather than a byte figure.
struct NameArena(UnsafeCell<[u8; NAME_ARENA_BYTES]>);

// SAFETY: every mutation reserves its region first with a `compare_exchange`
// on the matching `*_used` counter, so no two writers ever touch the same
// bytes; a region is written once and never rewritten or freed, so a reader
// holding a `&'static` into it can never observe a change. Publication to
// OTHER threads happens through the intern table's `AcqRel` CAS in
// `crate::get_or_create_logger`, which is the only way another thread can
// reach one of these references.
unsafe impl Sync for Pool {}

// SAFETY: as for `Pool` — every region is reserved by a CAS on
// `POOL.names_used` before it is written, and never written again.
unsafe impl Sync for NameArena {}

static POOL: Pool = Pool {
    loggers: UnsafeCell::new([const { MaybeUninit::uninit() }; CAPACITY]),
    loggers_used: AtomicUsize::new(0),
    names_used: AtomicUsize::new(0),
};

static NAMES: NameArena = NameArena(UnsafeCell::new([0_u8; NAME_ARENA_BYTES]));

/// Copy `name` into the static name arena and return a `&'static str` over the
/// copy. `None` if the name is too long or the arena is exhausted.
pub(crate) fn intern_name(name: &str) -> Option<&'static str> {
    let len = name.len();
    if len == 0 || len > MAX_LOGGER_NAME_LEN || len > NAME_ARENA_BYTES {
        return None;
    }
    loop {
        let used = POOL.names_used.load(Ordering::Acquire);
        let end = used.checked_add(len)?;
        if end > NAME_ARENA_BYTES {
            return None;
        }
        if POOL
            .names_used
            .compare_exchange(used, end, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            continue;
        }
        // SAFETY: `[used, end)` is reserved to this call by the CAS above and
        // is never written again. The arena is a `'static`, so the slice we
        // build over it lives for the program.
        let interned: &'static str = unsafe {
            let base = NAMES.0.get().cast::<u8>();
            core::ptr::copy_nonoverlapping(name.as_ptr(), base.add(used), len);
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(base.add(used), len))
        };
        return Some(interned);
    }
}

/// Move `logger` into the static logger arena and return a `&'static` to it.
/// `None` when the arena is exhausted.
pub(crate) fn place(logger: Logger) -> Option<&'static Logger> {
    loop {
        let used = POOL.loggers_used.load(Ordering::Acquire);
        if used >= CAPACITY {
            return None;
        }
        if POOL
            .loggers_used
            .compare_exchange(used, used + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            continue;
        }
        // SAFETY: slot `used` is reserved to this call by the CAS above, is
        // uninitialised until this write, and is never written again.
        let placed: &'static Logger = unsafe {
            let base = POOL.loggers.get().cast::<MaybeUninit<Logger>>();
            let slot = base.add(used);
            slot.write(MaybeUninit::new(logger));
            &*slot.cast::<Logger>()
        };
        return Some(placed);
    }
}

/// How many of the [`dynamic_logger_capacity`] slots are spent.
///
/// Counts slots CLAIMED, which is one more than the loggers reachable by name
/// whenever two threads raced the same new name: the loser's slot is spent and
/// unreferenced. Bounded and reported rather than reclaimed — this arena has
/// no free list by construction.
#[must_use]
pub fn dynamic_loggers_in_use() -> usize {
    POOL.loggers_used.load(Ordering::Acquire)
}

/// Bytes of the name arena spent, and its total size.
#[must_use]
pub fn dynamic_logger_name_arena() -> (usize, usize) {
    (POOL.names_used.load(Ordering::Acquire), NAME_ARENA_BYTES)
}

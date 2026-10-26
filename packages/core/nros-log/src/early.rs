//! Records that could not be delivered when they were raised: before any sink
//! was installed, or while another thread was inside a sink (issue 2000).
//!
//! ## The problem this exists for
//!
//! [`crate::init`] publishes the sink list. A record raised before that call
//! has nowhere to go, and for most of this crate's life it was constructed,
//! dispatched and DROPPED — silently, and invisibly to its author, who cannot
//! know what the board did before reaching their code.
//!
//! Issue 0708 answered by requiring every board boot funnel to call
//! `init_default()`, gated on funnels spelled `pub fn run*`. That is a SEARCH
//! for boot paths and it kept losing: NuttX's funnel is `pub extern "C" fn
//! nsh_main`, and three board crates did not link `nros-log` in the
//! configuration holding the funnel at all.
//!
//! Issue 0710 answered by having dispatch install the platform sink itself.
//! That removed the search — but it put `nros_platform_log_write` on a path
//! EVERY binary executes, which turned a pluggable delivery into a LINK-TIME
//! requirement. Seven test targets in `nros-rmw-cffi` and most of `nros-tests`
//! stopped linking under `check-workspace-features`, and no Cargo feature can
//! fix that: `nros-platform-cffi` and `nros-rmw-bridge` enable
//! `nros-node/rmw-cffi` unconditionally, so feature unification turns any
//! forwarded gate back ON for every member of a workspace build. A feature is a
//! property of the BUILD; what the question needs is a property of the BINARY.
//!
//! ## A funnel is not always a board method — issues 1048 and 1123
//!
//! The search above kept losing for one more reason, and it is worth naming
//! because it defeats the obvious grep: **not every boot funnel lives in a
//! board crate.** `nros::main!` emits a different entry shape per framework,
//! and two of them reach no `BoardEntry` method at all —
//!
//! * `Framework::Zephyr` (and its single-node sibling
//!   `nros::zephyr_component_main!`) emits `rust_main` directly, because Zephyr
//!   owns the C `main` and a Rust staticlib cannot take it over. That is why
//!   `nros-board-zephyr` has only `run_tiers` to install from, and why a
//!   single-tier pure-Rust Zephyr image published no sink list for two phases
//!   (issue 1123) while its `log::info!`s worked fine.
//! * a `[[bridge]]` system replaces the register/spin body with
//!   `nros_bridge::run_from_config_str`, so it reaches no board either.
//!
//! Both now call `nros_platform::log::init_default()` — reached through
//! `nros-platform` (which re-exports `nros_platform_cffi::log`) because that is
//! the crate a generated Entry and the Zephyr example leaves already declare;
//! giving each leaf a `nros-platform-cffi` dep of its own is the link-time
//! requirement 0710 rejected.
//!
//! So the enumeration is **board crates PLUS the framework arms of
//! `nros-macros`' `main_macro.rs` PLUS `nros`' entry macros**. Sweep it with:
//!
//! ```text
//! grep -rn 'Framework::[A-Za-z]* => ' packages/core/nros-macros/src/main_macro.rs
//! grep -rn 'init_default()' packages/boards packages/core/nros-macros packages/api/nros
//! ```
//!
//! ## What this does instead
//!
//! Hold the records. A record raised with no sinks installed is copied into a
//! bounded static ring here; [`crate::init`] drains it into whatever sinks the
//! board actually chose. Nothing is dropped, no board can forget, and this
//! crate touches no platform symbol to do it — the facade stays a facade.
//!
//! It is also STRICTLY better than installing a default sink was: the early
//! records land in the sink the board picked, rather than in whichever one
//! dispatch guessed before the board had spoken.
//!
//! ## Cost, and how to decline it
//!
//! `EARLY_DEPTH * (format buffer + name + header)` of static RAM, all of it in
//! `.bss`. The depth is the `NROS_LOG_EARLY_RECORDS` knob (issue 1037; builtin
//! 4, resolved by `build.rs` on the RFC-0049 ladder — it replaced the
//! `early-records-<N>` features), because a 64 KB MCU and a Linux host do not
//! want the same number. `0` declines the buffer entirely and restores the
//! pre-0708 behaviour of dropping, with the count below still kept so the loss
//! is at least reportable.
//!
//! Overflow is counted, never silently absorbed: [`overflowed`] returns how
//! many records did not fit, and the next delivery reports the count through
//! the sinks (`nros_log` logger, at `warn`).
//!
//! ## The same queue holds records that met a BUSY dispatcher — issue 2000
//!
//! Dispatch runs one record at a time: a flag is held across every
//! `sink.log()`, so a sink that logs cannot recurse into itself, and two
//! threads never write to a sink at once. Until issue 2000 a record that found
//! the flag held was DROPPED, which was right for the recursion it was written
//! for and wrong for everything else: a record from ANOTHER thread vanished
//! whenever any thread was mid-delivery. `nros-log` cannot tell the two apart
//! without a thread identity, which no `no_std` target here gives it for free.
//!
//! So such a record is held here instead, and the thread holding the flag
//! delivers it before letting go. Nobody waits and nothing interleaves inside
//! a sink. Two properties follow, both bounded on purpose:
//!
//! * **A sink that logs is not refused, it is postponed.** The holder makes at
//!   most [`DRAIN_PASSES`] passes, each over the records already held when the
//!   pass began, so a sink that logs on every write adds a bounded number of
//!   lines per dispatch and can never loop or grow the stack. What is left
//!   waits for the next dispatch or [`crate::flush`].
//! * **A record cannot be stranded by the hand-off.** The raiser publishes the
//!   record, then tries the flag once more; the holder releases the flag, then
//!   looks for held records once more. Under `SeqCst` one of the two always
//!   sees the other, so either the raiser delivers it or the holder does. The
//!   only record left behind is one raised during the holder's LAST pass, which
//!   the next dispatch or `flush` delivers.
//!
//! The knob sizes both uses, because they are one question: how many records
//! may be waiting for a sink at once.

use core::cell::UnsafeCell;

use portable_atomic::{AtomicU8, AtomicUsize, Ordering};

use crate::{Record, Severity, buffer::format_buffer_capacity};

/// Records held at once. See the module docs for the trade.
#[must_use]
pub const fn early_depth() -> usize {
    crate::config::EARLY_RECORDS
}

/// Passes a dispatcher makes over the held records before it lets go. See the
/// module docs: this is what bounds a sink that logs.
pub const DRAIN_PASSES: usize = 2;

const DEPTH: usize = early_depth();
const MSG_CAP: usize = format_buffer_capacity();
/// A logger name is an identifier, not prose — `nros-node`'s longest is 20.
const NAME_CAP: usize = 48;

/// Slot states. A slot cycles `EMPTY -> WRITING -> READY -> READING -> EMPTY`,
/// and each arrow is taken by exactly one thread (a CAS for the two claims).
const EMPTY: u8 = 0;
const WRITING: u8 = 1;
const READY: u8 = 2;
const READING: u8 = 3;

struct Pending {
    severity: Severity,
    logger_name: heapless::String<NAME_CAP>,
    message: heapless::String<MSG_CAP>,
    file: &'static str,
    line: u32,
    timestamp_ns: u64,
}

impl Pending {
    const fn new() -> Self {
        Self {
            severity: Severity::Info,
            logger_name: heapless::String::new(),
            message: heapless::String::new(),
            file: "",
            line: 0,
            timestamp_ns: 0,
        }
    }
}

struct Slot {
    /// Written only by the thread that moved `state` EMPTY -> WRITING, read
    /// only by the one that moved it READY -> READING.
    cell: UnsafeCell<Pending>,
    state: AtomicU8,
    /// Raise order, so a drain replays in the order records were raised.
    seq: AtomicUsize,
}

// SAFETY: `cell` has one owner at a time, handed over by `state`: a writer owns
// it from its EMPTY -> WRITING CAS to its `Release` store of READY, a reader
// from its READY -> READING CAS (`Acquire`) to its `Release` store of EMPTY.
unsafe impl Sync for Slot {}

impl Slot {
    const fn new() -> Self {
        Self {
            cell: UnsafeCell::new(Pending::new()),
            state: AtomicU8::new(EMPTY),
            seq: AtomicUsize::new(0),
        }
    }
}

#[allow(clippy::declare_interior_mutable_const)]
const EMPTY_SLOT: Slot = Slot::new();
static SLOTS: [Slot; DEPTH] = [EMPTY_SLOT; DEPTH];

/// Next raise-order number.
static NEXT_SEQ: AtomicUsize = AtomicUsize::new(0);
/// READY slots. `SeqCst` on both sides: it is half of the hand-off in the
/// module docs, paired with the dispatch flag.
static PENDING: AtomicUsize = AtomicUsize::new(0);
/// Records that found every slot taken. Never reset.
static LOST: AtomicUsize = AtomicUsize::new(0);

/// How many records did not fit and were lost, since the program started.
#[must_use]
pub fn overflowed() -> usize {
    LOST.load(Ordering::Relaxed)
}

/// Raise a record with `emit` until `delivered()` sees it, re-raising it
/// whenever [`overflowed`] counts a loss, for as long as `keep_waiting()`
/// says to. Returns whether `delivered()` saw it.
///
/// For a caller that must OBSERVE a record it raised — a test asserting on
/// what reached its sink. A record raised while another thread is delivering
/// is held, and one that does not fit is dropped and counted, so in a
/// multi-threaded test binary a record can be lost outright and no amount of
/// waiting or [`crate::flush`] delivers it. Measured on `nros-node`'s lib
/// tests: 8 copies at `--test-threads=64` failed 1 run in 48, with 16 records
/// counted lost, the asserted ones among them.
///
/// A counted loss may have been a sibling's record while the caller's was
/// still held, so a re-raise can deliver the record TWICE: assert on every
/// copy, never on a count. `emit` must therefore be safe to repeat — a
/// one-shot report (a latch it trips) has to reset its latch inside `emit`.
///
/// `keep_waiting` is the caller's budget and back-off, since this crate has
/// no clock: a hosted test passes a deadline that yields,
/// `|| { std::thread::yield_now(); Instant::now() < deadline }`.
pub fn emit_until_delivered(
    emit: impl Fn(),
    delivered: impl Fn() -> bool,
    mut keep_waiting: impl FnMut() -> bool,
) -> bool {
    loop {
        let lost = overflowed();
        emit();
        loop {
            crate::flush();
            if delivered() {
                return true;
            }
            if overflowed() != lost {
                break;
            }
            if !keep_waiting() {
                return delivered();
            }
        }
        if !keep_waiting() {
            return delivered();
        }
    }
}

/// Hold `record` until a dispatcher delivers it.
///
/// Returns `false` when it did not fit — the caller has nothing further to do,
/// but [`overflowed`] counts it and the next delivery reports it.
pub(crate) fn hold(record: &Record<'_>) -> bool {
    for slot in &SLOTS {
        if slot
            .state
            .compare_exchange(EMPTY, WRITING, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        slot.seq
            .store(NEXT_SEQ.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
        // SAFETY: the CAS above made this thread the slot's only owner until
        // the `Release` store of READY below.
        let pending = unsafe { &mut *slot.cell.get() };
        pending.severity = record.severity;
        // Truncating rather than refusing: a clipped record is worth more than
        // none, and the alternative is deciding that a long logger name loses
        // the whole line.
        pending.logger_name.clear();
        let _ = pending
            .logger_name
            .push_str(clip(record.logger_name, NAME_CAP));
        pending.message.clear();
        let _ = pending.message.push_str(clip(record.message, MSG_CAP));
        pending.file = record.file;
        pending.line = record.line;
        pending.timestamp_ns = record.timestamp_ns;
        slot.state.store(READY, Ordering::Release);
        PENDING.fetch_add(1, Ordering::SeqCst);
        return true;
    }
    LOST.fetch_add(1, Ordering::Relaxed);
    false
}

/// Whether any record is waiting.
pub(crate) fn pending() -> bool {
    PENDING.load(Ordering::SeqCst) > 0
}

/// Longest prefix of `s` that fits `cap` bytes without splitting a character.
fn clip(s: &str, cap: usize) -> &str {
    if s.len() <= cap {
        return s;
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// One pass: deliver every record held when the pass began, oldest first.
///
/// A record held DURING the pass waits for the next one — that is what bounds
/// a sink that logs (module docs). Safe to run concurrently with `hold` and
/// with another pass: a slot is claimed READY -> READING by one reader only.
pub(crate) fn drain_with(deliver: &mut dyn FnMut(&Record<'_>)) {
    let bound = NEXT_SEQ.load(Ordering::Acquire);
    loop {
        // Oldest READY slot raised before the pass began. DEPTH is a handful,
        // so a scan per record is cheaper than any ordered structure.
        let mut oldest: Option<(&Slot, usize)> = None;
        for slot in &SLOTS {
            if slot.state.load(Ordering::Acquire) != READY {
                continue;
            }
            let seq = slot.seq.load(Ordering::Relaxed);
            if seq >= bound {
                continue;
            }
            if oldest.is_none_or(|(_, best)| seq < best) {
                oldest = Some((slot, seq));
            }
        }
        let Some((slot, _)) = oldest else { break };
        if slot
            .state
            .compare_exchange(READY, READING, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Another pass took it first.
            continue;
        }
        // SAFETY: the CAS above made this the slot's only reader, and its
        // `Acquire` pairs with the writer's `Release` store of READY.
        let pending = unsafe { &*slot.cell.get() };
        let record = Record {
            severity: pending.severity,
            logger_name: pending.logger_name.as_str(),
            message: pending.message.as_str(),
            file: pending.file,
            line: pending.line,
            timestamp_ns: pending.timestamp_ns,
        };
        deliver(&record);
        slot.state.store(EMPTY, Ordering::Release);
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

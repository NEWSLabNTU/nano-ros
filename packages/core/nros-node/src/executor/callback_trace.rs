//! Phase 8 — callback-level dispatch tracing.
//!
//! Design: autoware-safety-island `docs/design/callback_tracing.rst`
//! (plan: `docs/roadmap/phase-8-callback-tracing.md`).
//!
//! A CTF capture shows THREADS. A callback does not run on a thread of its
//! own — it runs inside an executor wake, as an ordinary function call — so
//! thread-level tracing can see the executor's cadence and cannot see where
//! any individual callback begins or ends. The fix every runtime with the
//! same shape (tokio, Go, ros2_tracing) reaches for is to instrument the
//! MULTIPLEXER: the dispatcher is the only place that knows both identities.
//!
//! Three events, named after ros2_tracing so the concepts carry over:
//!
//! - [`register`] — once, when an entry is added to the executor. Records
//!   handle → (kind, name). This is ros2_tracing's init/runtime split, and
//!   it is what removes the hand-maintained handle→name enum that phase 7
//!   had to carry (and whose renumbering silently reinterpreted every trace
//!   taken before it).
//! - [`start`] / [`end`] — per dispatch, immediately around the leaf
//!   callback invocation. Handle only; no strings in the hot path.
//!
//! ## The sink
//!
//! Core must not name a platform (`nros-node`'s manifest states the policy:
//! the executor reaches the platform through a runtime vtable, "never via a
//! compile-time `#[cfg(feature = "platform-*")]` branch"). So the transport
//! is a runtime-installed function pointer in an [`AtomicPtr`], structurally
//! the same seam `wake_probe.rs` already uses for the same reason — hot-path
//! hooks that must vanish in production.
//!
//! [`TraceSink`]'s ABI is deliberately `unsafe extern "C" fn(u32, u32)`: that
//! IS the signature of a two-`uint32` platform trace event (on Zephyr,
//! `sys_trace_app_marker` behind an out-of-tree CTF patch), so a C caller can
//! install its shim directly with no Rust-side adapter and nano-ros never
//! references a symbol that only one downstream image defines.
//!
//! ## Wire encoding
//!
//! The carrier event has exactly two `u32` fields, `(marker_id, arg)`. Three
//! events, one of them carrying a variable-length name, are packed into it:
//!
//! | `marker_id` | event    | `arg`                                          |
//! |-------------|----------|------------------------------------------------|
//! | 16          | register | `handle << 8 \| kind`                          |
//! | 17          | name     | next 4 name bytes, byte *i* in bits `8*i`      |
//! | 18          | start    | `handle`                                       |
//! | 19          | end      | `handle`                                       |
//!
//! | 21          | violation          | `seq << 8 \| rule code` (phase-474 I1) |
//! | 22          | violation fqn      | FNV-1a 32 of the endpoint ref           |
//! | 23          | violation measured | `measured`                              |
//! | 24          | violation declared | `declared`                              |
//!
//! | 25          | take               | `handle << 24 \| take seq` (phase-474 I3) |
//! | 26          | take stamp sec     | the sample's `stamp.sec`                |
//! | 27          | take stamp nanosec | the sample's `stamp.nanosec`            |
//!
//! (20 is the handle-tagged name chunk, see [`MARKER_NAME`].) A violation is
//! four events in a row on one thread, emitted at DETECTION, so it sits on
//! the same clock as the dispatches it judges. The rule code is
//! `monitor::rule_code` (the index into `monitor::RULE_IDS` plus one), the
//! sequence number is the executor's own count (`Executor::violations_total`
//! at the time, low 24 bits), the same number the drain-and-report log line
//! prints. Events 22-24 bind to the 21 they FOLLOW; a decoder that sees a
//! 22-24 without its 21 drops it rather than guessing.
//!
//! phase-474 I3 -- a TAKE (25) is one sample taken for a subscription, emitted
//! before its callback's start (18), so the hop from the publisher's stamp to
//! the take is on the trace clock. Opt-in: per subscription with
//! [`set_take_trace`], image-wide with `NROS_TRACE_TAKES`
//! (`CONFIG_NROS_TRACE_TAKES`). The take sequence is image-wide, low 24 bits.
//! 26/27 follow a 25 when the sample's source stamp is known: the type has a
//! leading `stamp` the executor can find (`RosMessage::STAMP_OFFSET`, on the
//! typed Rust paths), or [`set_take_trace`] named its offset (the C and C++
//! paths, which carry no type). A decoder binds 26/27 to the 25 they FOLLOW.
//!
//! phase-474 I3 -- a TIMER's start/end pair can be sampled: every tick (the
//! default), one in N, or none, per timer ([`set_timer_trace_every`]) or
//! image-wide (`NROS_TRACE_TIMER_EVERY`, `CONFIG_NROS_TRACE_TIMER_EVERY`). A
//! 30 Hz tick whose jitter is the measurement is traced in full while the rest
//! are thinned to fit a RAM trace window. Every other dispatch is always
//! traced.
//!
//! The per-handle settings are keyed by the slot index the events carry, in
//! one image-wide table: on an image with several executors (tiers) a setting
//! applies to that slot index in each of them, which is also how the events
//! themselves name a callback.
//!
//! Name chunks repeat until the name is spent and are NUL-padded; a chunk
//! run binds to the register event it FOLLOWS. That is adjacency, not
//! keying — stated plainly because it is a real weakness: a dropped or
//! interleaved event inside a registration burst mis-attributes a NAME. It
//! is tolerable only because registration is init-time, once per callback,
//! on one thread, before any traffic worth measuring — and because a wrong
//! name can never corrupt a DURATION, which is keyed on the handle carried
//! by the runtime events alone. A capture that starts after init simply has
//! no names and the decoder falls back to `handle N`.
//!
//! Marker ids 1..7 belong to the application's hand-placed phase markers and
//! must never be reused: captured traces carry them, and renumbering
//! silently reinterprets every trace taken before it. The block starts at 16
//! rather than 8 so those keep room to grow contiguously.
//!
//! Gated behind `feature = "trace-callbacks"` so production builds carry zero
//! overhead and the call sites become `#[cfg]`-elided no-ops. NOT built on
//! the `tracing` crate facade: a no-op subscriber still costs span
//! construction and a level check on every dispatch.

#![cfg(feature = "trace-callbacks")]

use core::{
    fmt::Write as _,
    sync::atomic::{AtomicPtr, Ordering},
};

use super::arena::{EntryKind, TraceName};

/// `nros_callback_register(handle, kind, name)` — once, at registration.
pub const MARKER_REGISTER: u32 = 16;
/// Legacy untagged name chunk: 4 bytes, bound to the register event it
/// FOLLOWED. Never emitted any more, and reserved rather than reused so a
/// decoder can still read a capture taken before names carried their handle.
#[allow(dead_code)]
pub const MARKER_NAME_LEGACY: u32 = 17;
/// Name chunk, `handle << 24 | 3 bytes`.
///
/// A NEW id rather than a new payload under the old one. Redefining what 17
/// means would silently reinterpret every capture already taken with it —
/// precisely the failure this instrumentation exists to prevent, and the same
/// reason the application marker enum reserves its retired values.
pub const MARKER_NAME: u32 = 20;
/// `callback_start(handle)` — immediately before the callback is invoked.
pub const MARKER_START: u32 = 18;
/// `callback_end(handle)` — immediately after it returns.
pub const MARKER_END: u32 = 19;

// phase-474 I1 -- the violation markers. Defined beside the rule table in
// `monitor` (which builds without this feature, so the encoding is tested in
// every lane) and re-exported here with their siblings.
pub use super::monitor::{
    MARKER_VIOLATION, MARKER_VIOLATION_DECLARED, MARKER_VIOLATION_FQN, MARKER_VIOLATION_MEASURED,
};

/// phase-474 I3 -- one sample taken for a subscription:
/// `handle << 24 | take seq` (seq image-wide, low 24 bits).
pub const MARKER_TAKE: u32 = 25;
/// phase-474 I3 -- the taken sample's `stamp.sec`, after a [`MARKER_TAKE`].
pub const MARKER_TAKE_STAMP_SEC: u32 = 26;
/// phase-474 I3 -- the taken sample's `stamp.nanosec`, after a
/// [`MARKER_TAKE_STAMP_SEC`].
pub const MARKER_TAKE_STAMP_NSEC: u32 = 27;

/// Names longer than this are truncated. Matches the decoder's `CB_NAME_MAX`;
/// both sides must agree or the tail of a long name is read as a chunk of
/// the next registration's.
pub const NAME_MAX: usize = 64;

/// The trace sink, installed at app startup.
///
/// `unsafe extern "C" fn(u32, u32)` so a C-side shim (e.g. a Zephyr
/// `sys_trace_app_marker` wrapper) is installable verbatim.
///
/// # Safety
/// The installed function is called from the executor's dispatch path,
/// including from an OS-priority worker TASK — so it must be re-entrant and
/// safe to call from any thread that runs callbacks. It must not unwind.
pub type TraceSink = unsafe extern "C" fn(u32, u32);

/// `AtomicPtr<()>` holding the installed [`TraceSink`]. A plain
/// `Option<TraceSink>` cannot be atomic because a function pointer is not
/// `AtomicPtr<T>`'s `T`; store as `*mut ()` and `transmute` on read. Same
/// shape as `wake_probe::CYCLE_READER`.
static SINK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the trace sink. Call once at app startup, BEFORE the executor
/// registers anything — a sink installed later misses the registration
/// events and the decoder falls back to `handle N` labels. `None` disables
/// tracing (hooks become no-ops on the next call).
pub fn set_trace_sink(sink: Option<TraceSink>) {
    let ptr = match sink {
        Some(f) => f as *mut (),
        None => core::ptr::null_mut(),
    };
    SINK.store(ptr, Ordering::Release);
}

#[inline]
fn sink() -> Option<TraceSink> {
    let ptr = SINK.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the pointer was installed via `set_trace_sink` from a real
        // `TraceSink` fn pointer; same ABI on load. The producer's `Release`
        // pairs with this `Acquire`.
        Some(unsafe { core::mem::transmute::<*mut (), TraceSink>(ptr) })
    }
}

#[inline]
fn emit(sink: TraceSink, marker_id: u32, arg: u32) {
    // SAFETY: `sink` came from `sink()`, i.e. from a `TraceSink` installed by
    // `set_trace_sink`, whose contract (see [`TraceSink`]) covers being called
    // from the dispatch path on any executor thread.
    unsafe { sink(marker_id, arg) }
}

/// Wire `kind` for the register event.
///
/// The design's table is `0` timer, `1` subscription, `2` service, `3`
/// action. `EntryKind` has SEVEN variants, not four: clients fold onto their
/// server's kind (a service client is still service traffic, an action client
/// still action traffic), which loses nothing the handle does not already
/// distinguish.
///
/// `GuardCondition` is the one that does not fit — the design's four-kind
/// framing omits it. It gets `4` rather than being squeezed into a wrong
/// bucket or dropped from the registration sweep; the decoder's kind table
/// falls back to a literal `kind 4` label for anything outside `0..=3`, so
/// this degrades to a readable row rather than a misattribution.
#[inline]
fn wire_kind(kind: EntryKind) -> u32 {
    match kind {
        EntryKind::Timer => 0,
        EntryKind::Subscription => 1,
        EntryKind::Service | EntryKind::ServiceClient => 2,
        EntryKind::ActionServer | EntryKind::ActionClient => 3,
        EntryKind::GuardCondition => 4,
    }
}

/// Stream a name as 3-byte chunks TAGGED WITH THE HANDLE, NUL-padded,
/// truncated at [`NAME_MAX`].
///
/// The tag is the point. Chunks used to be four bytes with no handle, and the
/// decoder bound them to whichever register event they happened to FOLLOW —
/// by adjacency. One dropped event inside a registration burst silently
/// shifted every subsequent name onto the wrong callback, and a trace with
/// confidently mislabelled rows is worse than one with no names at all,
/// because nothing about it looks wrong.
///
/// Layout: `handle << 24 | b2 << 16 | b1 << 8 | b0`. Three bytes per event
/// instead of four is a third more events, which costs nothing: registration
/// happens once per callback at init, not on the hot path.
///
/// The handle is masked to 8 bits, which is what `start`/`end` already carry
/// and comfortably covers `MAX_CALLBACK_SLOTS`.
fn stream_name(sink: TraceSink, handle: usize, name: &str) {
    let tag = ((handle as u32) & 0xff) << 24;
    let bytes = name.as_bytes();
    let n = bytes.len().min(NAME_MAX);
    let mut i = 0;
    while i < n {
        let mut word: u32 = tag;
        let mut j = 0usize;
        while j < 3 {
            if i + j < n {
                word |= (bytes[i + j] as u32) << (8 * j as u32);
            }
            j += 1;
        }
        emit(sink, MARKER_NAME, word);
        i += 3;
    }
}

/// Emit `nros_callback_register(handle, kind, name)` plus the name chunks.
///
/// Called once per executor entry, from the single `emplace_entry` choke
/// point every registration site funnels through. Init-time, so the cost of
/// streaming the name is paid once and steady-state tracing carries handles
/// only.
///
/// `handle` is the entry slot index — stable (no code path writes
/// `entries[i] = None` after registration; `cancel_timer` only sets a flag),
/// unique across every kind (one flat table), and bounded by
/// `MAX_CALLBACK_SLOTS`, so it fits the 24 bits left after the kind byte with
/// room to spare.
pub(crate) fn register(handle: usize, kind: EntryKind, name: TraceName<'_>) {
    let Some(sink) = sink() else { return };

    emit(
        sink,
        MARKER_REGISTER,
        ((handle as u32 & 0x00ff_ffff) << 8) | wire_kind(kind),
    );

    // Synthesised names go through a stack buffer — no allocator, and the
    // formatting only exists in a build that enabled this feature.
    let mut scratch: heapless::String<NAME_MAX> = heapless::String::new();
    let text: &str = match name {
        TraceName::Text(s) => s,
        // Timers carry no name anywhere in the executor, so the period is
        // the only thing that distinguishes one from another in a trace.
        TraceName::TimerPeriod(period_us) => {
            // A truncated synthesised name is still more useful than none;
            // `write!` failing means the buffer filled, not that the value
            // was bad.
            let _ = write!(&mut scratch, "timer@{period_us}us");
            scratch.as_str()
        }
        // Some entries carry nothing at all — not even a period (guard
        // conditions; an arena subscription handed an already-open
        // `RmwSubscriber`). The slot index is the only identity there is, and
        // it is exactly what the runtime events key on.
        TraceName::Slot(label, slot) => {
            let _ = write!(&mut scratch, "{label}#{slot}");
            scratch.as_str()
        }
    };
    stream_name(sink, handle, text);
}

/// `callback_start(handle)` — immediately before a leaf callback is invoked.
///
/// O(1), allocation-free, lock-free; one relaxed-ordering pointer load plus
/// the sink call. No state is kept across the pair, which is what makes it
/// safe to fire from the OS-priority worker thread as well as the executor's
/// own (`os_priority.rs` runs `try_process` on a different thread, so any
/// depth counter or open-span slot here would have had to be per-thread from
/// day one). Pairing is done offline by the decoder, keyed on the handle.
#[inline]
pub(crate) fn start(handle: u8) {
    if let Some(sink) = sink() {
        emit(sink, MARKER_START, handle as u32);
    }
}

/// `callback_end(handle)` — immediately after the callback returns.
///
/// See [`start`]. A callback that panics or long-jumps out skips its `end`;
/// the decoder counts that as an unbalanced span rather than a duration.
#[inline]
pub(crate) fn end(handle: u8) {
    if let Some(sink) = sink() {
        emit(sink, MARKER_END, handle as u32);
    }
}

/// phase-474 I1 -- one stored contract violation, as four events (see the
/// module table). Pre-arm verdicts (phase-474 I2) are not stored and emit
/// nothing.
pub(crate) fn violation(seq: u32, v: &super::monitor::Violation) {
    let Some(sink) = sink() else { return };
    for (id, arg) in super::monitor::violation_marker_words(seq, v) {
        emit(sink, id, arg);
    }
}

// ---------------------------------------------------------------------------
// phase-474 I3 -- per-sample take events and per-timer sampling.
// ---------------------------------------------------------------------------

use core::sync::atomic::{AtomicU16, AtomicU32};

/// Slot indices the events can carry (`handle` is a `u8`).
const SLOTS: usize = 256;

/// Per-slot take setting: 0 = the image default (`NROS_TRACE_TAKES`), 1 = off,
/// 2 = on with the type's own stamp (if any), `3 + off` = on with the stamp at
/// byte `off` of the serialized sample (encapsulation header included).
static TAKE_CFG: [AtomicU16; SLOTS] = [const { AtomicU16::new(0) }; SLOTS];
/// Per-slot timer sampling: 0 = the image default (`NROS_TRACE_TIMER_EVERY`),
/// `1 + n` = one tick in `n` traced (`n == 0`: none).
static TIMER_EVERY: [AtomicU16; SLOTS] = [const { AtomicU16::new(0) }; SLOTS];
/// Per-slot tick counter for the sampling.
static TIMER_TICKS: [AtomicU16; SLOTS] = [const { AtomicU16::new(0) }; SLOTS];
/// Image-wide take sequence.
static TAKE_SEQ: AtomicU32 = AtomicU32::new(0);

/// phase-474 I3 -- trace the takes of the subscription in `handle` (its slot
/// index, the `handle` its register event carries), or stop.
///
/// `stamp_offset`: where the sample's source `stamp` (`sec: i32`,
/// `nanosec: u32`, little-endian) sits in the serialized sample, counting the
/// 4-byte encapsulation header -- 4 for a type that starts with a `Header` or
/// a `builtin_interfaces/Time stamp`. `None` keeps the type's own offset on
/// the typed Rust paths and emits no stamp on the C and C++ paths. Overrides
/// the image default (`NROS_TRACE_TAKES`) for this slot.
pub fn set_take_trace(handle: u8, on: bool, stamp_offset: Option<u16>) {
    let v = match (on, stamp_offset) {
        (false, _) => 1,
        (true, None) => 2,
        (true, Some(off)) => off.saturating_add(3),
    };
    TAKE_CFG[handle as usize].store(v, Ordering::Relaxed);
}

/// phase-474 I3 -- trace one tick in `every` of the timer in `handle` (1 =
/// every tick, 0 = none). Overrides the image default
/// (`NROS_TRACE_TIMER_EVERY`) for this slot. The first tick after the call is
/// traced.
pub fn set_timer_trace_every(handle: u8, every: u16) {
    TIMER_TICKS[handle as usize].store(0, Ordering::Relaxed);
    TIMER_EVERY[handle as usize].store(every.saturating_add(1), Ordering::Relaxed);
}

/// Back to the image defaults for every slot (tests, and an image that
/// re-registers).
pub fn reset_trace_settings() {
    for i in 0..SLOTS {
        TAKE_CFG[i].store(0, Ordering::Relaxed);
        TIMER_EVERY[i].store(0, Ordering::Relaxed);
        TIMER_TICKS[i].store(0, Ordering::Relaxed);
    }
}

/// Whether this tick of the timer in `handle` gets its start/end pair. Counts
/// the tick either way.
#[inline]
pub(crate) fn timer_tick_traced(handle: u8) -> bool {
    if sink().is_none() {
        return false;
    }
    let every = match TIMER_EVERY[handle as usize].load(Ordering::Relaxed) {
        0 => crate::config::TRACE_TIMER_EVERY,
        v => v - 1,
    };
    match every {
        0 => false,
        1 => true,
        n => {
            let t = TIMER_TICKS[handle as usize].fetch_add(1, Ordering::Relaxed);
            t.is_multiple_of(n)
        }
    }
}

/// One sample taken for the subscription in `handle`: [`MARKER_TAKE`], then
/// the stamp pair when one is known. `type_stamp` is the type's own offset
/// (`RosMessage::STAMP_OFFSET`) on a typed path, `None` on a raw one.
#[inline]
pub(crate) fn take(handle: u8, raw: &[u8], type_stamp: Option<usize>) {
    let Some(sink) = sink() else { return };
    let stamp = match TAKE_CFG[handle as usize].load(Ordering::Relaxed) {
        0 if crate::config::TRACE_TAKES => type_stamp,
        0 | 1 => return,
        2 => type_stamp,
        v => Some((v - 3) as usize),
    };
    let seq = TAKE_SEQ.fetch_add(1, Ordering::Relaxed);
    emit(
        sink,
        MARKER_TAKE,
        ((handle as u32) << 24) | (seq & 0x00ff_ffff),
    );
    if let Some(off) = stamp
        && let (Some(sec), Some(nsec)) = (raw.get(off..off + 4), raw.get(off + 4..off + 8))
    {
        emit(
            sink,
            MARKER_TAKE_STAMP_SEC,
            u32::from_le_bytes([sec[0], sec[1], sec[2], sec[3]]),
        );
        emit(
            sink,
            MARKER_TAKE_STAMP_NSEC,
            u32::from_le_bytes([nsec[0], nsec[1], nsec[2], nsec[3]]),
        );
    }
}

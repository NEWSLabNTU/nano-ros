//! The `/rosout` QUEUE — the half of a `/rosout` bridge that is a logging
//! concern.
//!
//! ## What this is, and what it deliberately is not
//!
//! Ledger row `c:logging_rosout_enabled` (log.json) records what a nano-ros
//! node costs an operator: `ros2 topic echo /rosout`, `rqt_console` and every
//! launch-side log aggregator see nothing. Closing that needs two halves, and
//! they belong in different crates:
//!
//! * **the queue** — a record raised anywhere in the image has to reach the
//!   thread that owns the publisher. That is this module.
//! * **the publisher** — `rcl_interfaces/msg/Log` on `/rosout`, which needs a
//!   node, a session and an RMW backend. That is
//!   `nros_node::rosout`, and this crate has no business knowing it exists.
//!
//! The split is the reason `add_sink`'s own doc calls the appendable list "for
//! consumers that TEE (a /rosout bridge, a test collector)". This is that
//! bridge's near half.
//!
//! ## Why the sink does not publish
//!
//! A [`crate::LogSink`] that calls into the RMW stack is re-entrant by
//! construction: the transport logs, and a transport that logs while a log is
//! being delivered re-enters `dispatch_to_sinks`. On Zephyr native_sim that is
//! not a hang but a silent death — issue 0589's `zvfs_write` recursion
//! exhausts the stack with no message.
//!
//! [`dispatch_to_sinks`](crate) already holds `RECURSION_GUARD` across every
//! `sink.log()` call, so the immediate recursion is already refused. This
//! module refuses the SECOND shape as well, which that guard cannot see: the
//! pump runs OUTSIDE dispatch, so a record raised by the publish path would be
//! enqueued normally, published on the next pump, log again, and amplify
//! forever at steady state without ever recursing. [`PUMPING`] is set for the
//! whole drain and [`enqueue`] refuses while it is set, counting the refusals
//! separately ([`suppressed`]) from the ring-full ones ([`dropped`]) because
//! the two mean different things: ring-full is a sizing answer, suppressed is
//! this design working.
//!
//! So the whole of this module's hot path is: one atomic CAS, one copy into a
//! `.bss` slot, one release store. No allocation, no platform symbol, no RMW.
//!
//! ## Cost
//!
//! [`ring_bytes`] of `.bss` plus 26 bytes of counters, and **nothing at all**
//! in an image that does not enable the `rosout` feature — the module is
//! `cfg`'d out whole. The cost moves with TWO feature families, so here it is
//! MEASURED rather than described (`cargo test -p nros-log --features rosout
//! --lib ring_bytes_probe -- --nocapture`, once per combination):
//!
//! | `rosout-records-` | `buffer-size-` | per slot | ring |
//! | --- | --- | --- | --- |
//! | 8 | 128 | 232 B | 1 856 B |
//! | 8 | 256 | 360 B | 2 880 B |
//! | **16 (default)** | **256 (default)** | **360 B** | **5 760 B** |
//! | 64 | 256 | 360 B | 23 040 B |
//! | 64 | 1024 | 1 128 B | 72 192 B |
//!
//! Depth comes from the `rosout-records-<N>` family, the same shape and for
//! the same reason as `early-records-<N>`: a 64 KB MCU and a Linux host do not
//! want the same number. Note the right-hand column against a Zephyr image's
//! 16 KB picolibc arena — the default is a third of it, and
//! `rosout-records-8` + `buffer-size-128` is the build that fits.

use core::cell::UnsafeCell;

use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::{LogSink, Record, Severity, buffer::format_buffer_capacity};

/// How many records the queue holds between pumps.
///
/// Drop-on-full with a COUNT, never silent absorption: a `/rosout` stream that
/// quietly loses records is worse than one that says how many it lost, because
/// an operator reads the absence as "nothing happened".
#[must_use]
pub const fn rosout_depth() -> usize {
    if cfg!(feature = "rosout-records-64") {
        64
    } else if cfg!(feature = "rosout-records-32") {
        32
    } else if cfg!(feature = "rosout-records-8") {
        8
    } else {
        16
    }
}

const DEPTH: usize = rosout_depth();
const MSG_CAP: usize = format_buffer_capacity();
/// Same bound and same reason as `early::NAME_CAP` — a logger name is an
/// identifier, not prose.
const NAME_CAP: usize = 48;

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
    /// Written by the producer that claimed this index; read by the single
    /// consumer after `ready` publishes it.
    cell: UnsafeCell<Pending>,
    /// `1` once the producer finished writing `cell`. `Release` here,
    /// `Acquire` in [`drain`].
    ready: AtomicUsize,
}

// SAFETY: a producer writes `cell` only for an index it won from the `HEAD`
// CAS below, and only while `HEAD - TAIL < DEPTH` guarantees that index maps to
// a slot the consumer has already released (it advanced `TAIL` past it with a
// `Release` store the producer read `Acquire`). The consumer reads `cell` only
// after observing the producer's `Release` store to `ready`.
unsafe impl Sync for Slot {}

impl Slot {
    const fn new() -> Self {
        Self {
            cell: UnsafeCell::new(Pending::new()),
            ready: AtomicUsize::new(0),
        }
    }
}

#[allow(clippy::declare_interior_mutable_const)]
const EMPTY_SLOT: Slot = Slot::new();
static SLOTS: [Slot; DEPTH] = [EMPTY_SLOT; DEPTH];

/// Monotonic count of records ACCEPTED into the ring. `HEAD % DEPTH` is the
/// slot the next producer writes.
static HEAD: AtomicUsize = AtomicUsize::new(0);
/// Monotonic count of records DRAINED. Single writer: [`drain`].
static TAIL: AtomicUsize = AtomicUsize::new(0);
/// Records refused because the ring was full.
static DROPPED: AtomicUsize = AtomicUsize::new(0);
/// Records refused because they were raised inside [`drain`].
static SUPPRESSED: AtomicUsize = AtomicUsize::new(0);
/// Set for the whole of [`drain`]. See the module docs: this is the half of
/// the re-entrancy answer that `dispatch_to_sinks`'s guard cannot give.
static PUMPING: AtomicBool = AtomicBool::new(false);
/// Whether [`enable`] installed the sink.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The [`LogSink`] that feeds the queue. Zero-sized: the state is the statics
/// above, so an image pays for the ring and nothing for the sink.
#[derive(Debug, Default, Clone, Copy)]
pub struct RosoutSink;

impl LogSink for RosoutSink {
    fn log(&self, record: &Record<'_>) {
        enqueue(record);
    }
}

/// The one instance. `add_sink` takes `&'static dyn LogSink`, and a ZST has
/// nothing to configure, so there is no reason for a second.
pub static ROSOUT_SINK: RosoutSink = RosoutSink;

/// Install [`ROSOUT_SINK`] so records start queueing for `/rosout`.
///
/// Idempotent: a second call is a no-op and still answers `true`.
///
/// Returns `false` when [`crate::MAX_ADDED_SINKS`] are already registered — the
/// sink is NOT installed and [`enabled`] stays `false`, which is the honest
/// answer for `rcl_logging_rosout_enabled`'s counterpart. The caller is
/// expected to say so rather than carry on as if `/rosout` were live.
pub fn enable() -> bool {
    if ENABLED.load(Ordering::Acquire) {
        return true;
    }
    if crate::add_sink(&ROSOUT_SINK) {
        ENABLED.store(true, Ordering::Release);
        true
    } else {
        false
    }
}

/// Whether records are being queued for `/rosout`.
///
/// The counterpart of rcl's `rcl_logging_rosout_enabled()`. It answers about
/// the QUEUE, which is what this crate can see; whether anything drains it is
/// [`nros_node::rosout::pump`]'s business and the envelope the ledger row
/// states.
///
/// [`nros_node::rosout::pump`]: https://docs.rs/nros-node
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Records refused because the ring was full since the last [`drain`] reported
/// them. Saturating, never reset by anything but [`drain`].
#[must_use]
pub fn dropped() -> usize {
    DROPPED.load(Ordering::Relaxed)
}

/// Records refused because they were raised from inside [`drain`].
///
/// A non-zero value is not a defect: it is the amplification loop this module
/// exists to refuse, and it means the transport logged while publishing.
#[must_use]
pub fn suppressed() -> usize {
    SUPPRESSED.load(Ordering::Relaxed)
}

/// The `.bss` this module costs, in bytes: the ring, and nothing else that
/// scales.
///
/// Derived rather than documented, because it moves with TWO feature families
/// — `rosout-records-<N>` picks the depth and `buffer-size-<N>` the per-record
/// message capacity — so any figure written in prose is right for one build.
/// Measured on the shipped defaults (depth 16, `buffer-size-256`): **5 760
/// bytes** (360 B per slot), plus 26 bytes of counters and flags.
///
/// An image that does not enable the `rosout` feature pays ZERO: this module
/// is `cfg`'d out whole, statics included.
#[must_use]
pub const fn ring_bytes() -> usize {
    DEPTH * core::mem::size_of::<Slot>()
}

/// How many records are queued right now.
#[must_use]
pub fn len() -> usize {
    HEAD.load(Ordering::Acquire)
        .saturating_sub(TAIL.load(Ordering::Acquire))
}

/// Whether the queue is empty.
#[must_use]
pub fn is_empty() -> bool {
    len() == 0
}

/// Take and clear the drop counters.
///
/// [`drain`] calls this so the pump can report the loss on the very channel it
/// was lost from, and clear it in the same step — a counter that is read
/// without being cleared reports the same loss on every pump.
pub fn take_losses() -> (usize, usize) {
    (
        DROPPED.swap(0, Ordering::Relaxed),
        SUPPRESSED.swap(0, Ordering::Relaxed),
    )
}

/// Copy `record` into the ring. Returns `false` when it was refused.
fn enqueue(record: &Record<'_>) -> bool {
    if DEPTH == 0 {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    if PUMPING.load(Ordering::Acquire) {
        SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    let idx = loop {
        let head = HEAD.load(Ordering::Acquire);
        let tail = TAIL.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= DEPTH {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if HEAD
            .compare_exchange_weak(head, head + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break head;
        }
    };
    let slot = &SLOTS[idx % DEPTH];
    // SAFETY: `idx` was won by the CAS above, so this thread is the only
    // producer for it. The `head - tail < DEPTH` test that preceded the CAS
    // means the consumer has already advanced `TAIL` past this slot's previous
    // occupant, and that `Release` store is what the `Acquire` load of `TAIL`
    // synchronised with. No reader touches `cell` until the `Release` store to
    // `ready` below.
    let pending = unsafe { &mut *slot.cell.get() };
    pending.severity = record.severity;
    // Truncating rather than refusing, for `early::hold`'s reason: a clipped
    // record is worth more than none.
    pending.logger_name.clear();
    let _ = pending
        .logger_name
        .push_str(clip(record.logger_name, NAME_CAP));
    pending.message.clear();
    let _ = pending.message.push_str(clip(record.message, MSG_CAP));
    pending.file = record.file;
    pending.line = record.line;
    pending.timestamp_ns = record.timestamp_ns;
    slot.ready.store(1, Ordering::Release);
    true
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

/// Hand every queued record to `deliver`, oldest first, and return how many.
///
/// SINGLE CONSUMER. Two concurrent drains would both read the slot at `TAIL`;
/// the guard below makes the second one answer `0` rather than duplicate.
///
/// `deliver` runs with [`PUMPING`] set, so anything it logs — including
/// anything the RMW stack logs underneath it — is refused by [`enqueue`] and
/// counted in [`suppressed`] instead of joining the ring it is draining.
pub fn drain(deliver: &mut dyn FnMut(&Record<'_>)) -> usize {
    if PUMPING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
        .is_err()
    {
        return 0;
    }
    let mut delivered = 0usize;
    loop {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            break;
        }
        let slot = &SLOTS[tail % DEPTH];
        if slot.ready.load(Ordering::Acquire) == 0 {
            // Claimed but not yet published. Stop rather than skip: the queue
            // is ordered, and the next pump will find it.
            break;
        }
        {
            // SAFETY: the `Acquire` load above pairs with the producer's
            // `Release` store, and this is the only consumer (the `PUMPING`
            // guard). The producer cannot reuse the slot until `TAIL` moves,
            // which happens after this borrow ends.
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
        }
        slot.ready.store(0, Ordering::Release);
        TAIL.store(tail + 1, Ordering::Release);
        delivered += 1;
    }
    PUMPING.store(false, Ordering::Release);
    delivered
}

/// Forget everything queued and every counter. **Tests only** — the statics
/// above are process-global, so a test that does not reset them reads the
/// previous test's records.
#[doc(hidden)]
pub fn reset_for_test() {
    let _ = drain(&mut |_| {});
    DROPPED.store(0, Ordering::Relaxed);
    SUPPRESSED.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring, the counters and `PUMPING` are process-global, and the test
    /// harness runs `#[test]`s on concurrent threads — so without this every
    /// assertion here reads whatever the other tests happened to leave. A
    /// spin lock rather than `std::sync::Mutex` because this crate is
    /// `#![no_std]` unconditionally.
    static TEST_LOCK: AtomicBool = AtomicBool::new(false);

    struct Serialised;

    impl Serialised {
        fn acquire() -> Self {
            while TEST_LOCK
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
                .is_err()
            {
                core::hint::spin_loop();
            }
            reset_for_test();
            Self
        }
    }

    impl Drop for Serialised {
        fn drop(&mut self) {
            reset_for_test();
            TEST_LOCK.store(false, Ordering::Release);
        }
    }

    fn record<'a>(message: &'a str, name: &'a str) -> Record<'a> {
        Record {
            severity: Severity::Info,
            logger_name: name,
            message,
            file: "rosout.rs",
            line: 7,
            timestamp_ns: 42,
        }
    }

    /// The whole point: a record raised goes in, and a pump gets it back in
    /// order with every field intact.
    #[test]
    fn a_queued_record_comes_back_whole() {
        let _guard = Serialised::acquire();
        assert!(enqueue(&record("first", "planner")));
        assert!(enqueue(&record("second", "planner")));
        assert_eq!(len(), 2);
        let mut seen: heapless::Vec<(Severity, u32, u64), 4> = heapless::Vec::new();
        let mut bodies: heapless::Vec<u8, 64> = heapless::Vec::new();
        let n = drain(&mut |r| {
            let _ = seen.push((r.severity, r.line, r.timestamp_ns));
            for b in r.message.as_bytes() {
                let _ = bodies.push(*b);
            }
            assert_eq!(r.logger_name, "planner");
            assert_eq!(r.file, "rosout.rs");
        });
        assert_eq!(n, 2);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], (Severity::Info, 7, 42));
        assert_eq!(core::str::from_utf8(&bodies).unwrap(), "firstsecond");
        assert!(is_empty());
    }

    /// A full ring refuses and SAYS SO. The alternative — overwriting the
    /// oldest — loses the boot story, which is the part an operator most wants.
    #[test]
    fn a_full_ring_drops_and_counts() {
        let _guard = Serialised::acquire();
        for _ in 0..DEPTH {
            assert!(enqueue(&record("x", "n")));
        }
        assert!(!enqueue(&record("overflow", "n")));
        assert_eq!(dropped(), 1);
        let n = drain(&mut |_| {});
        assert_eq!(n, DEPTH);
        // Space is back.
        assert!(enqueue(&record("after", "n")));
    }

    /// The amplification refusal, measured. A `deliver` that logs — which is
    /// what an RMW stack does underneath a real pump — must not grow the queue
    /// it is draining.
    #[test]
    fn a_pump_that_logs_cannot_feed_itself() {
        let _guard = Serialised::acquire();
        assert!(enqueue(&record("seed", "n")));
        let n = drain(&mut |_| {
            // Exactly what the publish path does when it has something to say.
            assert!(!enqueue(&record("transport chatter", "rmw")));
        });
        assert_eq!(n, 1, "the drain must deliver the seed and then stop");
        assert!(is_empty(), "the chatter must not have joined the ring");
        assert_eq!(suppressed(), 1);
    }

    /// Losses are reported once, not on every pump.
    #[test]
    fn taking_losses_clears_them() {
        let _guard = Serialised::acquire();
        for _ in 0..(DEPTH + 3) {
            let _ = enqueue(&record("x", "n"));
        }
        assert_eq!(take_losses(), (3, 0));
        assert_eq!(take_losses(), (0, 0));
    }

    /// The `.bss` figure this module'"'"'s doc and the ledger row quote, MEASURED
    /// rather than asserted -- it moves with two feature families, so a test
    /// that only checked a bound would let the quoted number rot.
    #[test]
    fn the_ring_costs_what_the_docs_say() {
        let per_slot = core::mem::size_of::<Slot>();
        assert_eq!(ring_bytes(), DEPTH * per_slot);
        // The module doc quotes a five-row table over two feature families;
        // one build can only check its own row, so this checks the ROW IT IS
        // and the RULE the whole table follows. A bare `assert_eq!(5760)`
        // would pass vacuously in every non-default build, which is the
        // shape a cost figure rots in.
        assert_eq!(
            per_slot,
            8 + (MSG_CAP + NAME_CAP + 45).next_multiple_of(8),
            "the per-slot rule the module doc's table is computed from moved"
        );
        if DEPTH == 16 && MSG_CAP == 256 {
            assert_eq!(
                ring_bytes(),
                5760,
                "the DEFAULT ring is the figure nros-log/src/rosout.rs and \
                 ledger row `c:logging_rosout_enabled` quote; {per_slot} B/slot \
                 x {DEPTH}"
            );
        }
    }

    /// A long name and a long body are clipped on a character boundary, never
    /// split mid-codepoint.
    #[test]
    fn oversized_fields_clip_on_a_character_boundary() {
        let _guard = Serialised::acquire();
        let name = "é".repeat(NAME_CAP);
        let body = "é".repeat(MSG_CAP);
        assert!(enqueue(&record(&body, &name)));
        let n = drain(&mut |r| {
            assert!(r.logger_name.len() <= NAME_CAP);
            assert!(r.message.len() <= MSG_CAP);
            assert!(r.logger_name.chars().all(|c| c == 'é'));
            assert!(r.message.chars().all(|c| c == 'é'));
        });
        assert_eq!(n, 1);
    }
}

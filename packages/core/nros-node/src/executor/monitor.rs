//! RFC-0052 / phase-296 W3b.4/.5 — on-target contract monitors.
//!
//! The baked shape mirrors Phase 211.H's `qos_overrides`: codegen emits a
//! `&'static [MonitorSpec]` table (plus one `static PubMonitorCell` per
//! contracted publisher) from the SystemModel's contract layer; the entry
//! installs it on the executor before entity creation. An uncontracted
//! image bakes an empty table — every path below dead-code-eliminates.
//!
//! Publish counting is an atomic bump on the publisher handle (no clock,
//! no lock on the hot path); the rate check runs on spin ticks over a
//! ~[`RATE_CHECK_INTERVAL_US`] window and pushes violations into a small
//! ring the entry glue drains into the `nros-diagnostics` reporter.
//!
//! W3b.5 adds three more rules on the same drain:
//! - `max-age-runtime` — subscriber take-age (`epoch_now - header.stamp`
//!   peeked from the raw CDR buffer at `RosMessage::STAMP_OFFSET`,
//!   recorded into a [`SubMonitorCell`] on the take path).
//! - `max-latency-runtime` — node-path (take → publish) latency: the
//!   dispatch elapsed time is attributed to every monitored publisher
//!   whose counter advanced during that dispatch (an upper bound on
//!   take → publish, measured on the executor's monotonic clock).
//! - `deadline-miss-runtime` — a dispatched callback ran past its bound
//!   SchedContext's `deadline_us`; what ELSE happens is the tier's
//!   [`DeadlineAction`](super::sched_context::DeadlineAction).
//!
//! phase-462 W2 adds one more on the same drain, and it is the other half of
//! what the contract's `on_violation` lowers to:
//! - `silence-runtime` -- a contracted subscription that took NOTHING for a
//!   whole `max_age_ms` window (`check_silence`). A violation is either "the
//!   callback ran too long", which is the deadline action, or "the input
//!   stopped coming", which is this; the four rules above can only see the
//!   first kind.

use core::sync::atomic::Ordering;
// portable-atomic: RMW ops (fetch_add/fetch_max/swap) exist even on
// riscv32imc / Cortex-M0+ that lack native CAS (same choice as
// `SporadicState` / `AtomicSporadicState` in sched_context.rs).
use portable_atomic::AtomicU32;

/// One contracted publisher's counters. Baked as a `static` by codegen
/// (or declared by the fixture); the publisher handle bumps `count` on
/// every publish, the executor reads deltas on spin ticks.
#[derive(Debug, Default)]
pub struct PubMonitorCell {
    pub count: AtomicU32,
    /// W3b.5 — max observed take→publish latency (µs) in the current
    /// check window. Written by the dispatch loop (fetch_max), drained
    /// (swap 0) by the latency check.
    pub max_latency_us: AtomicU32,
    /// Age of the stamp this publisher last put ON THE WIRE, in
    /// microseconds: `epoch_now - outgoing header.stamp`.
    ///
    /// Distinct from `max_latency_us`, which times this node's own
    /// take→publish work. This says how old the DATA is that the node just
    /// published, which is the quantity a chain is made of.
    ///
    /// It exists to answer a question `max-age-runtime` cannot. That rule
    /// measures `epoch_now - stamp` on the TAKE path, so if every node in a
    /// chain propagates the original stamp -- the usual ROS convention, each
    /// node copying its input's stamp to its output -- the age at the final
    /// consumer already IS the end-to-end latency. If any node re-stamps
    /// with `now`, the clock silently resets and the same number becomes
    /// single-hop age instead. Same units, same magnitude, no warning.
    ///
    /// A publish age near zero on a node that consumes input is the
    /// signature of re-stamping. Recording it here is what lets a chain's
    /// provenance be checked at all, rather than assumed.
    ///
    /// `0` = never observed, matching the other cells: the type has no
    /// `STAMP_OFFSET`, or no epoch source is installed.
    pub last_publish_stamp_age_us: AtomicU32,
}

impl PubMonitorCell {
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            max_latency_us: AtomicU32::new(0),
            last_publish_stamp_age_us: AtomicU32::new(0),
        }
    }
}

/// One contracted subscriber's take-age accumulator (W3b.5). The take
/// path records `epoch_now - header.stamp` per message (fetch_max); the
/// age check drains it (swap 0) per window.
#[derive(Debug, Default)]
pub struct SubMonitorCell {
    /// Max observed take-age (ms) in the current check window.
    pub max_age_ms: AtomicU32,
    /// phase-462 W2 -- observations recorded since the last check, drained
    /// (swap 0) by `check_age`. The age itself cannot answer "did anything
    /// arrive": a window with no take and a window whose only take was
    /// perfectly fresh both leave `max_age_ms` at 0.
    pub takes: AtomicU32,
}

impl SubMonitorCell {
    pub const fn new() -> Self {
        Self {
            max_age_ms: AtomicU32::new(0),
            takes: AtomicU32::new(0),
        }
    }

    /// Take-path hook: record one message's age. `stamp_us` is the
    /// peeked `header.stamp` as µs since the UNIX epoch, `epoch_now_us`
    /// the receive-side wall clock. A stamp from the future clamps to 0.
    ///
    /// phase-462 W2 -- also the silence rule's only evidence that this
    /// endpoint is being fed. It counts what the AGE rule can see, which is
    /// stamped takes on a type that carries a stamp: a contracted endpoint
    /// whose messages carry no readable stamp records nothing here and is
    /// reported silent. That is the intended verdict rather than a gap --
    /// its `max_age_ms` cannot be judged either, and an unjudgeable promise
    /// reported as met is the silent class this phase exists to remove.
    pub fn observe(&self, stamp_us: u64, epoch_now_us: u64) {
        let age_ms = (epoch_now_us.saturating_sub(stamp_us) / 1_000).min(u32::MAX as u64) as u32;
        self.max_age_ms.fetch_max(age_ms, Ordering::Relaxed);
        self.takes.fetch_add(1, Ordering::Relaxed);
    }
}

/// Peek `Time { i32 sec; u32 nanosec }` little-endian at `offset` in a
/// raw CDR receive buffer (encapsulation header included) and return µs
/// since the UNIX epoch. `None` when the buffer is too short or the
/// stamp is pre-epoch/zero (unstamped messages never fire age monitors).
/// Record the age of the stamp a publisher just put on the wire.
///
/// Called from the publish path with the encoded CDR still in hand, using the
/// same `STAMP_OFFSET` peek the take path uses. A no-op when the type carries
/// no stamp, when no epoch source is installed, or when the publisher is
/// uncontracted -- the same three ways `observe_age` folds away.
///
/// Stores rather than accumulates: this is "how old was the last thing
/// published", a state, not a window maximum. A chain check wants the current
/// value, and a max would be pinned forever by one stale message at startup.
#[inline]
pub fn observe_publish_stamp(cell: &PubMonitorCell, raw: &[u8], offset: usize, now_us: u64) {
    if let Some(stamp_us) = peek_stamp_us(raw, offset) {
        let age = now_us.saturating_sub(stamp_us).min(u32::MAX as u64) as u32;
        cell.last_publish_stamp_age_us.store(age, Ordering::Relaxed);
    }
}

pub fn peek_stamp_us(raw: &[u8], offset: usize) -> Option<u64> {
    let sec_b = raw.get(offset..offset + 4)?;
    let nsec_b = raw.get(offset + 4..offset + 8)?;
    let sec = i32::from_le_bytes([sec_b[0], sec_b[1], sec_b[2], sec_b[3]]);
    let nsec = u32::from_le_bytes([nsec_b[0], nsec_b[1], nsec_b[2], nsec_b[3]]);
    if sec <= 0 {
        return None;
    }
    Some(sec as u64 * 1_000_000 + nsec as u64 / 1_000)
}

/// One monitored publisher endpoint.
#[derive(Debug, Clone, Copy)]
pub struct MonitorSpec {
    /// Topic name EXACTLY as the node passes it to `create_publisher`
    /// (the SystemModel's wiring carries the same resolved name).
    pub topic: &'static str,
    /// Endpoint ref for violation reports (`<node FQN>/<endpoint>` — the
    /// SystemModel contract key).
    pub fqn: &'static str,
    /// Declared publisher guarantee, in milli-Hz (fixed point: Hz × 1000).
    /// 0 = no rate contract on this endpoint.
    pub min_rate_hz_milli: u32,
    /// W3b.5 — node-path budget (ms) for paths whose OUTPUT is this
    /// endpoint (`contracts.node_paths[..].max_latency_ms`). 0 = no
    /// latency contract.
    pub max_latency_ms: u32,
    /// The endpoint's counter cell.
    pub cell: &'static PubMonitorCell,
}

/// One monitored subscriber endpoint (W3b.5 age contracts). Separate
/// table from [`MonitorSpec`] — sub contracts key different endpoints
/// and need no publish counter.
#[derive(Debug, Clone, Copy)]
pub struct AgeMonitorSpec {
    /// Topic name EXACTLY as the node passes it to `create_subscription`.
    pub topic: &'static str,
    /// Endpoint ref for violation reports (the SystemModel contract key).
    pub fqn: &'static str,
    /// Declared max take-age (ms). 0 = no age contract.
    pub max_age_ms: u32,
    /// The endpoint's age accumulator.
    pub cell: &'static SubMonitorCell,
}

/// phase-436 E4 — how often the stack-headroom rule may query the port, in µs.
///
/// On a painted Zephyr stack the query walks every UNUSED byte from the stack
/// base (`z_stack_space_get`), so querying on every spin put that scan inside
/// the control loop. The high-water mark only grows, so a throttled check sees
/// the same low mark and only sees it later.
pub(crate) const STACK_HEADROOM_CHECK_INTERVAL_US: u64 = 1_000_000;

/// The same limit for a build with no clock: once per this many spins.
pub(crate) const STACK_HEADROOM_CHECK_SPIN_STRIDE: u32 = 1_000;

/// Whether the stack-headroom query is due.
///
/// `last_check_us` is `None` until the first check, which is immediate, so a
/// bound set at boot is checked on the first spin. After that a clocked build
/// waits one full interval and a clockless build one full stride of spins.
pub(crate) fn stack_headroom_check_due(
    now_us: Option<u64>,
    last_check_us: Option<u64>,
    spins_since_check: u32,
) -> bool {
    match (last_check_us, now_us) {
        (None, _) => true,
        (Some(last), Some(now)) => now.saturating_sub(last) >= STACK_HEADROOM_CHECK_INTERVAL_US,
        (Some(_), None) => spins_since_check >= STACK_HEADROOM_CHECK_SPIN_STRIDE,
    }
}

/// Rate-check window (µs). Matches play_launch's ~5 s time-based trigger
/// so both runtimes converge on comparable cadence.
pub const RATE_CHECK_INTERVAL_US: u64 = 5_000_000;

/// Max RATE/LATENCY-monitored endpoints per executor (inline table, no_std).
///
/// phase-467 W1 (issue 1471) -- generated, not a literal: the
/// `NROS_EXECUTOR_MAX_MONITORS` knob, whose derived rung is the image's own
/// `monitor_rows` count (the sizing descriptor's `[image] monitor_rows`), 8 when nothing
/// states or derives it. A table longer than this is REFUSED at install
/// ([`check_table_capacity`]), never truncated: the spin loop inspects only the
/// first `MAX_MONITORS` specs, so a truncating install would boot with the rest
/// of the contract silently unwatched.
pub const MAX_MONITORS: usize = crate::config::MAX_MONITORS;
/// Max AGE-monitored subscriptions per executor -- the second table, on its
/// own knob (`NROS_EXECUTOR_MAX_AGE_MONITORS`, derived from `age_rows`) so an
/// image with rate contracts and no age contracts pays for one table only.
pub const MAX_AGE_MONITORS: usize = crate::config::MAX_AGE_MONITORS;

/// A monitor table longer than the executor can watch -- phase-467 W1.
///
/// Its `Display` NAMES THE KNOB to raise (RFC-0065 D2, refuse and name the
/// remedy), so every installer that refuses reports the same words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorTableFull {
    /// `"NROS_EXECUTOR_MAX_MONITORS"` or `"NROS_EXECUTOR_MAX_AGE_MONITORS"`.
    pub knob: &'static str,
    /// Rows the table carries.
    pub rows: usize,
    /// Rows this build can watch (the knob's resolved value).
    pub capacity: usize,
}

impl core::fmt::Display for MonitorTableFull {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "monitor table has {} rows but this executor watches {}; raise {} \
             (it derives from the contract's row count when nothing states it)",
            self.rows, self.capacity, self.knob
        )
    }
}

/// Whether a table of `n_rows` rate/latency rows and `n_ages` age rows fits
/// this build's executor. The rate table is judged first.
pub fn check_table_capacity(n_rows: usize, n_ages: usize) -> Result<(), MonitorTableFull> {
    check_capacity_against(n_rows, n_ages, MAX_MONITORS, MAX_AGE_MONITORS)
}

/// [`check_table_capacity`] against explicit capacities, so the refusal is
/// testable at any knob value without rebuilding the crate.
pub fn check_capacity_against(
    n_rows: usize,
    n_ages: usize,
    max_rows: usize,
    max_ages: usize,
) -> Result<(), MonitorTableFull> {
    if n_rows > max_rows {
        return Err(MonitorTableFull {
            knob: "NROS_EXECUTOR_MAX_MONITORS",
            rows: n_rows,
            capacity: max_rows,
        });
    }
    if n_ages > max_ages {
        return Err(MonitorTableFull {
            knob: "NROS_EXECUTOR_MAX_AGE_MONITORS",
            rows: n_ages,
            capacity: max_ages,
        });
    }
    Ok(())
}
/// Violation ring depth, per executor, and the slot count of the image-wide
/// SWD record ([`ViolationRecord`]).
///
/// phase-474 I1 -- `NROS_EXECUTOR_MAX_VIOLATIONS` (Zephyr:
/// `CONFIG_NROS_EXECUTOR_MAX_VIOLATIONS`), default 8. The ring keeps the
/// LATEST this-many verdicts, so the depth bounds how much recent history a
/// reader sees and never whether a late violation is kept at all.
pub const MAX_VIOLATIONS: usize = crate::config::MAX_VIOLATIONS;

/// A detected contract violation, in the play_launch rule-id vocabulary.
#[derive(Debug, Clone)]
pub struct Violation {
    /// `"rate-hierarchy-runtime"` | `"max-age-runtime"` |
    /// `"max-latency-runtime"` | `"deadline-miss-runtime"` |
    /// `"stack-headroom-runtime"` | `"alive-supervision-runtime"` |
    /// `"silence-runtime"` (phase-462 W2).
    pub rule: &'static str,
    /// Violating endpoint ref (from the spec's `fqn`; the SC name for
    /// deadline misses).
    pub fqn: &'static str,
    /// Measured value. Unit is per-rule: milli-Hz for the rate rule, ms
    /// for age/latency, µs for deadline misses, dropped activations for
    /// the timer-overrun rule.
    pub measured: u32,
    /// Declared bound, same unit as `measured`.
    pub declared: u32,
}

/// phase-474 I1 -- every rule id the executor reports, in WIRE ORDER: a rule's
/// numeric code is its index here plus one, and 0 means "not a rule this table
/// knows".
///
/// The code is what a trace marker and the SWD record carry, because neither
/// can carry a string. Append only: a capture or a RAM dump taken before a
/// reorder would silently decode as the wrong rule, which is the class the
/// callback-trace marker ids are frozen against (`callback_trace.rs`).
pub const RULE_IDS: [&str; 9] = [
    "rate-hierarchy-runtime",
    "max-age-runtime",
    "max-latency-runtime",
    "deadline-miss-runtime",
    "stack-headroom-runtime",
    "alive-supervision-runtime",
    "silence-runtime",
    "timer-overrun-runtime",
    "release-jitter-runtime",
];

/// The wire code of `rule` ([`RULE_IDS`] index + 1), or 0 for a rule the
/// table does not know.
pub fn rule_code(rule: &str) -> u32 {
    RULE_IDS
        .iter()
        .position(|r| *r == rule)
        .map(|i| i as u32 + 1)
        .unwrap_or(0)
}

/// The rule a wire code names, or `None` for 0 / an unknown code.
pub fn rule_name(code: u32) -> Option<&'static str> {
    RULE_IDS.get((code as usize).checked_sub(1)?).copied()
}

/// 32-bit FNV-1a of an endpoint ref -- the identity a trace marker and the SWD
/// record carry for `Violation::fqn` (a string neither can hold).
///
/// FNV-1a because a decoder can recompute it in one line from the model's
/// endpoint list (`scripts/read-violation-record.py` does), with no table
/// baked into the image.
pub const fn fqn_hash(fqn: &str) -> u32 {
    let b = fqn.as_bytes();
    let mut h: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < b.len() {
        h ^= b[i] as u32;
        h = h.wrapping_mul(0x0100_0193);
        i += 1;
    }
    h
}

/// phase-474 I1 -- trace marker: a stored violation, `seq << 8 | rule code`.
/// The id block continues `callback_trace`'s 16-20; ids 1-7 are the
/// application's and are never used here.
pub const MARKER_VIOLATION: u32 = 21;
/// phase-474 I1 -- trace marker: the endpoint ref's [`fqn_hash`].
pub const MARKER_VIOLATION_FQN: u32 = 22;
/// phase-474 I1 -- trace marker: `measured`.
pub const MARKER_VIOLATION_MEASURED: u32 = 23;
/// phase-474 I1 -- trace marker: `declared`.
pub const MARKER_VIOLATION_DECLARED: u32 = 24;

/// phase-474 I1 -- the four `(marker_id, arg)` events one stored violation
/// becomes in a trace (`callback_trace` emits them, in this order, when the
/// `trace-callbacks` feature is on and a sink is installed). `seq` is the
/// executor's sequence number; its low 24 bits ride in the first event.
pub fn violation_marker_words(seq: u32, v: &Violation) -> [(u32, u32); 4] {
    [
        (
            MARKER_VIOLATION,
            ((seq & 0x00ff_ffff) << 8) | (rule_code(v.rule) & 0xff),
        ),
        (MARKER_VIOLATION_FQN, fqn_hash(v.fqn)),
        (MARKER_VIOLATION_MEASURED, v.measured),
        (MARKER_VIOLATION_DECLARED, v.declared),
    ]
}

/// `"NRVR"` -- nano-ros violation record.
pub const RECORD_MAGIC: u32 = 0x4e52_5652;
/// [`ViolationRecord`] layout version. Bump on any field change; a reader
/// refuses a version it does not know (`scripts/read-violation-record.py`).
pub const RECORD_VERSION: u32 = 1;
/// `u32` words in one [`ViolationSlot`].
pub const RECORD_SLOT_WORDS: u32 = 7;
/// `u32` words in the [`ViolationRecord`] header, before the first slot.
pub const RECORD_HEADER_WORDS: u32 = 10;

/// One slot of the SWD record. Every field is a `u32` on every target, so the
/// layout is the same on a 32-bit board and a 64-bit host.
#[repr(C)]
#[derive(Debug)]
pub struct ViolationSlot {
    /// 1-based sequence number of the violation in this slot, 0 = empty.
    /// Written LAST (zeroed first), so a reader that sees a non-zero `seq`
    /// twice around a read of the other words has a whole entry.
    pub seq: AtomicU32,
    /// [`rule_code`].
    pub rule: AtomicU32,
    /// [`fqn_hash`] of the endpoint ref.
    pub fqn_hash: AtomicU32,
    pub measured: AtomicU32,
    pub declared: AtomicU32,
    /// Address of the endpoint ref's bytes (exact on a 32-bit target, the low
    /// half of it on a 64-bit host) and its length: the text is in the
    /// image's rodata, so a reader with the ELF can print the name.
    pub fqn_addr: AtomicU32,
    pub fqn_len: AtomicU32,
}

impl ViolationSlot {
    pub const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            rule: AtomicU32::new(0),
            fqn_hash: AtomicU32::new(0),
            measured: AtomicU32::new(0),
            declared: AtomicU32::new(0),
            fqn_addr: AtomicU32::new(0),
            fqn_len: AtomicU32::new(0),
        }
    }
}

impl Default for ViolationSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// phase-474 I1 -- the image-wide violation record a debugger reads by NAME,
/// for a board whose console reaches nobody.
///
/// The executor's ring is carved out of executor storage (no symbol, and a
/// drain empties it), so on the Autoware Safety Island it could only be found
/// by scanning RAM for rule-id string pointers, and it held the FIRST eight
/// verdicts since boot. This record is the black box beside it: a `#[repr(C)]`
/// static, every word a `u32`, never drained, keeping the LATEST
/// [`MAX_VIOLATIONS`] stored violations of every executor in the image, with
/// the running total.
///
/// Layout (version [`RECORD_VERSION`], little-endian `u32` words):
///
/// | word | field | meaning |
/// | --- | --- | --- |
/// | 0 | `magic` | [`RECORD_MAGIC`] (`"NRVR"`) |
/// | 1 | `version` | [`RECORD_VERSION`] |
/// | 2 | `capacity` | slots (`NROS_EXECUTOR_MAX_VIOLATIONS`) |
/// | 3 | `slot_words` | [`RECORD_SLOT_WORDS`] |
/// | 4 | `total` | violations stored since boot (the last `seq`) |
/// | 5 | `head` | slot the NEXT violation takes (`total % capacity`) |
/// | 6 | `dropped` | entries overwritten (`total - capacity`, saturating) |
/// | 7 | `suppressed_before_arm` | verdicts before the monitors armed (phase-474 I2) |
/// | 8 | `armed` | executors whose monitors are armed (phase-474 I2) |
/// | 9 | `reserved` | 0 |
/// | 10.. | `slots` | `capacity` x [`ViolationSlot`] |
///
/// The newest entry is in slot `(head + capacity - 1) % capacity`; reading
/// `capacity` slots backwards from there gives newest to oldest, and a slot
/// whose `seq` is 0 was never written.
///
/// Present only when the image asked for the boot report
/// (`NROS_BOOT_REPORT=1`, Zephyr `CONFIG_NROS_BOOT_REPORT=y`) -- the same
/// opt-in as the SWD boot record it sits beside, so an image that does not
/// opt in is unchanged. Read it with `scripts/read-violation-record.py`.
#[repr(C)]
#[derive(Debug)]
pub struct ViolationRecord<const N: usize> {
    pub magic: AtomicU32,
    pub version: AtomicU32,
    pub capacity: AtomicU32,
    pub slot_words: AtomicU32,
    pub total: AtomicU32,
    pub head: AtomicU32,
    pub dropped: AtomicU32,
    pub suppressed_before_arm: AtomicU32,
    pub armed: AtomicU32,
    pub reserved: AtomicU32,
    pub slots: [ViolationSlot; N],
}

impl<const N: usize> ViolationRecord<N> {
    /// A valid, empty record: the header is written at compile time, so a
    /// record with `total == 0` reads as "no violation since boot" rather than
    /// as uninitialised RAM.
    pub const fn new() -> Self {
        Self {
            magic: AtomicU32::new(RECORD_MAGIC),
            version: AtomicU32::new(RECORD_VERSION),
            capacity: AtomicU32::new(N as u32),
            slot_words: AtomicU32::new(RECORD_SLOT_WORDS),
            total: AtomicU32::new(0),
            head: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            suppressed_before_arm: AtomicU32::new(0),
            armed: AtomicU32::new(0),
            reserved: AtomicU32::new(0),
            slots: [const { ViolationSlot::new() }; N],
        }
    }

    /// Store one violation, overwriting the oldest when full. Returns its
    /// 1-based sequence number.
    ///
    /// Lock-free and callable from any executor thread: the slot is claimed by
    /// a `fetch_add` on `total`, so two executors never write one slot unless
    /// `N` more violations land during one write.
    pub fn store(&self, v: &Violation) -> u32 {
        let seq = self.total.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        if N == 0 {
            return seq;
        }
        let slot = &self.slots[(seq as usize - 1) % N];
        slot.seq.store(0, Ordering::Release);
        slot.rule.store(rule_code(v.rule), Ordering::Relaxed);
        slot.fqn_hash.store(fqn_hash(v.fqn), Ordering::Relaxed);
        slot.measured.store(v.measured, Ordering::Relaxed);
        slot.declared.store(v.declared, Ordering::Relaxed);
        slot.fqn_addr
            .store(v.fqn.as_ptr() as usize as u32, Ordering::Relaxed);
        slot.fqn_len.store(v.fqn.len() as u32, Ordering::Relaxed);
        slot.seq.store(seq, Ordering::Release);
        self.head.store(seq % N as u32, Ordering::Relaxed);
        self.dropped
            .store(seq.saturating_sub(N as u32), Ordering::Relaxed);
        seq
    }

    /// The stored entries, newest first, as `(seq, rule code, fqn hash,
    /// measured, declared)`. For tests and on-target self-checks; a debugger
    /// reads the words directly.
    pub fn newest_first(&self, mut f: impl FnMut(u32, u32, u32, u32, u32)) {
        let total = self.total.load(Ordering::Acquire);
        let n = (total as usize).min(N);
        for k in 0..n {
            let seq = total - k as u32;
            let slot = &self.slots[(seq as usize - 1) % N];
            if slot.seq.load(Ordering::Acquire) != seq {
                continue; // overwritten or mid-write
            }
            f(
                seq,
                slot.rule.load(Ordering::Relaxed),
                slot.fqn_hash.load(Ordering::Relaxed),
                slot.measured.load(Ordering::Relaxed),
                slot.declared.load(Ordering::Relaxed),
            );
        }
    }
}

impl<const N: usize> Default for ViolationRecord<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// phase-474 I1 -- THE record, by name: `nm zephyr.elf | grep
/// NROS_VIOLATION_RECORD`. See [`ViolationRecord`] for the layout.
#[cfg(nros_boot_report)]
#[unsafe(no_mangle)]
pub static NROS_VIOLATION_RECORD: ViolationRecord<MAX_VIOLATIONS> = ViolationRecord::new();

/// Issue 1635 — where an executor hands its drained violations when the image
/// asked it to (`Executor::set_violation_sink`).
///
/// A function and an opaque context rather than a closure, because the
/// executor is not generic and the reporter it reaches lives in a crate above
/// this one (`nros-cpp` publishes on `/diagnostics` through
/// `nros-diagnostics`). Called at DETECTION, from inside the executor's spin
/// (never from inside a user callback) — so the sink may publish.
///
/// # Safety
/// The sink is called with the `ctx` it was installed with; whoever installs it
/// guarantees `ctx` is valid for every call until the sink is replaced or the
/// executor is dropped.
pub type ViolationSink = unsafe fn(ctx: *mut core::ffi::c_void, v: &Violation);

/// phase-474 I1 -- everything an executor does with a detected violation, in
/// one place: the switches, the sink, the ring and its counters.
///
/// The ring keeps the LATEST [`MAX_VIOLATIONS`] verdicts: a push into a full
/// ring evicts the oldest and counts it in `dropped`. It used to refuse the
/// push instead, so a board whose start-up filled the ring stored nothing
/// after it (the Autoware Safety Island's W31 bring-up: 8 of 8 slots, all
/// start-up). `total` counts every verdict stored since boot, so
/// `total - dropped - ring.len()` is how many a drain has already taken.
pub(crate) struct ViolationChannel<'s> {
    /// Issue 0514 -- log each violation at detection.
    pub(crate) report: bool,
    /// phase-474 I1 -- drain the ring at the end of every spin and report each
    /// entry with its sequence number and the counters
    /// ([`Executor::set_violation_drain_report`]). Replaces the log at
    /// detection while on, so one verdict is one line.
    ///
    /// [`Executor::set_violation_drain_report`]: super::Executor::set_violation_drain_report
    pub(crate) drain_report: bool,
    /// Issue 1635 -- the image's reporter, fed at detection. The context is a
    /// `usize` so the executor keeps its auto traits; it is the installer's
    /// pointer.
    pub(crate) sink: Option<(ViolationSink, usize)>,
    /// phase-409 -- CARVED, at `MAX_VIOLATIONS` (no `ExecutorSizing` knob:
    /// the build-time depth is the capability).
    pub(crate) ring: super::storage::CarvedVec<'s, Violation>,
    /// Violations this executor stored since boot (wrapping). It is also the
    /// sequence number of the newest entry in `ring`: every stored verdict is
    /// pushed, so the ring holds `total - len + 1 ..= total`.
    pub(crate) total: u32,
    /// Issue 0514 / phase-474 I1 -- entries evicted from the full ring before
    /// any drain took them (saturating).
    pub(crate) dropped: u32,
}

impl<'s> ViolationChannel<'s> {
    pub(crate) fn new(ring: super::storage::CarvedVec<'s, Violation>) -> Self {
        Self {
            report: true,
            drain_report: crate::config::VIOLATION_DRAIN_REPORT,
            sink: None,
            ring,
            total: 0,
            dropped: 0,
        }
    }

    /// Issue 1635 / phase-474 I1 -- THE one place a detected violation goes:
    /// the log floor (issue 0514) unless the drain hook reports instead, the
    /// image's sink when one is installed, the trace marker when callback
    /// tracing is compiled in, the SWD record when the image keeps one, and
    /// the ring [`drain_violations`] reads.
    ///
    /// [`drain_violations`]: super::Executor::drain_violations
    pub(crate) fn record(&mut self, v: Violation) {
        if self.report && !self.drain_report {
            log_violation(&v);
        }
        if let Some((f, ctx)) = self.sink {
            // SAFETY: `Executor::set_violation_sink`'s contract — `ctx` is valid
            // for every call until the sink is replaced or the executor dropped.
            unsafe { f(ctx as *mut core::ffi::c_void, &v) };
        }
        // `total` is this executor's sequence number for the verdict: the log
        // line and the trace marker carry it, so the two name one verdict by
        // one number. The SWD record numbers across every executor in the
        // image; on a single-executor image the two coincide.
        self.total = self.total.wrapping_add(1);
        #[cfg(nros_boot_report)]
        NROS_VIOLATION_RECORD.store(&v);
        #[cfg(feature = "trace-callbacks")]
        super::callback_trace::violation(self.total, &v);
        if self.ring.capacity() == 0 {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        if self.ring.len() == self.ring.capacity() {
            self.ring.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        // Cannot fail: a slot was just made if the ring was full.
        let _ = self.ring.push(v);
    }

    /// Hand every entry still in the ring to `f`, oldest first, with its
    /// sequence number, and empty the ring.
    pub(crate) fn drain(&mut self, mut f: impl FnMut(u32, &Violation)) {
        let n = self.ring.len() as u32;
        let first = self.total.wrapping_sub(n).wrapping_add(1);
        for (k, v) in self.ring.iter().enumerate() {
            f(first.wrapping_add(k as u32), v);
        }
        self.ring.clear();
    }

    /// phase-474 I1 -- the drain-and-report hook: one log line per entry with
    /// its sequence number and the counters, so a reader of the log knows how
    /// many it missed. Called at the end of a spin when `drain_report` is on.
    pub(crate) fn drain_and_report(&mut self) {
        if !self.drain_report || self.ring.is_empty() {
            return;
        }
        let (total, dropped) = (self.total, self.dropped);
        self.drain(|seq, v| log_drained_violation(seq, total, dropped, v));
    }
}

/// Per-spec accounting state (parallel to the spec table).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MonitorState {
    /// Window opened (a plain bool, not a 0-sentinel on the timestamp —
    /// `now_us == 0` is a legitimate first sample on freshly-started
    /// monotonic clocks).
    pub(crate) opened: bool,
    pub(crate) window_start_us: u64,
    pub(crate) count_at_window_start: u32,
    /// Suppress duplicate reports: only re-report after a clean window.
    pub(crate) violated_last_window: bool,
    /// W3b.5 — separate dedup for the latency rule on the same spec row.
    pub(crate) latency_violated_last_window: bool,
}

/// Pure rate check over one window boundary. Returns `Some(violation)`
/// when the window elapsed AND the measured rate is below the declared
/// minimum (and we didn't already report last window).
///
/// Extracted from the executor so the math is unit-testable without a
/// session: publish counting is injected via the cell, time via `now_us`.
pub(crate) fn check_rate(
    spec: &MonitorSpec,
    state: &mut MonitorState,
    now_us: u64,
) -> Option<Violation> {
    if spec.min_rate_hz_milli == 0 {
        return None;
    }
    let count = spec.cell.count.load(Ordering::Relaxed);
    if !state.opened {
        // First observation: open the window, no verdict yet.
        state.opened = true;
        state.window_start_us = now_us;
        state.count_at_window_start = count;
        return None;
    }
    let window_us = now_us.saturating_sub(state.window_start_us);
    if window_us < RATE_CHECK_INTERVAL_US {
        return None;
    }
    let published = count.wrapping_sub(state.count_at_window_start) as u64;
    // milli-Hz = published * 1e3 / window_s = published * 1e9 / window_us
    let measured_milli_hz =
        (published.saturating_mul(1_000_000_000) / window_us.max(1)).min(u32::MAX as u64) as u32;

    // Roll the window.
    state.window_start_us = now_us;
    state.count_at_window_start = count;

    if measured_milli_hz < spec.min_rate_hz_milli {
        if state.violated_last_window {
            return None; // still violated — already reported
        }
        state.violated_last_window = true;
        Some(Violation {
            rule: "rate-hierarchy-runtime",
            fqn: spec.fqn,
            measured: measured_milli_hz,
            declared: spec.min_rate_hz_milli,
        })
    } else {
        state.violated_last_window = false;
        None
    }
}

/// Pure latency check: drains the spec cell's window-max take→publish
/// latency and fires when it exceeds the declared node-path budget.
/// Same report-once-until-recovery semantics as the rate rule; runs on
/// every monitor tick (the cell accumulates between ticks, so no window
/// bookkeeping is needed — draining IS the window roll).
pub(crate) fn check_latency(spec: &MonitorSpec, state: &mut MonitorState) -> Option<Violation> {
    if spec.max_latency_ms == 0 {
        return None;
    }
    let max_us = spec.cell.max_latency_us.swap(0, Ordering::Relaxed);
    let max_ms = max_us / 1_000;
    if max_ms > spec.max_latency_ms {
        if state.latency_violated_last_window {
            return None;
        }
        state.latency_violated_last_window = true;
        Some(Violation {
            rule: "max-latency-runtime",
            fqn: spec.fqn,
            measured: max_ms,
            declared: spec.max_latency_ms,
        })
    } else {
        // A quiet window (no dispatch attributed) also counts as clean —
        // recovery resets the dedup like the rate rule's clean window.
        state.latency_violated_last_window = false;
        None
    }
}

/// Per-age-spec dedup state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AgeState {
    pub(crate) violated_last_window: bool,
    /// phase-462 W2 -- the silence rule's window. `opened` is a plain bool for
    /// the same reason [`MonitorState`]'s is: `now_us == 0` is a legitimate
    /// first sample.
    pub(crate) silence_opened: bool,
    /// Report-once-until-recovery, matching every other rule here.
    pub(crate) silence_reported: bool,
    /// Monotonic MILLISECONDS of the last check that saw a take (or of the
    /// first check, which starts the lease rather than judging it).
    ///
    /// Milliseconds and `u32`, not the uss the clock hands out, because this
    /// array is INLINE in the `Executor` value (`[AgeState; MAX_AGE_MONITORS]`) and
    /// that value has a byte budget a knob-scaled table must not blow (issue
    /// 0961 / `storage.rs`'s `the_executor_value_does_not_scale_with_the_knobs`
    /// -- a `u64` here costs 8 rows x 16 B and fails it). The unit is the
    /// contract's own (`max_age_ms`), the comparison is a wrapping delta, so
    /// the 49-day rollover is not a discontinuity, and a silence window is
    /// never shorter than a millisecond.
    pub(crate) last_take_ms: u32,
}

/// phase-462 W2 -- the on-target liveliness lease: a contracted subscription
/// that has taken NOTHING for a whole `max_age_ms` window.
///
/// The bound needs no new declaration and deliberately does not invent one.
/// `max_age_ms` already says how old this input's data may be; data that never
/// arrives is older than that by the same clock, so the window IS the declared
/// bound. (`on_violation` selects what the executor then DOES about it; the
/// reaction beyond reporting is the fault hook phase-462 W3 owns.)
///
/// Why this is not `max-age-runtime` with a longer arm: that rule judges a
/// message that arrived. An input that stops entirely produces no message to
/// judge, so it leaves the age rule silent forever -- the exact failure a
/// `max_age_ms` promise is made against, and the one play_launch's reaction
/// engine catches on the Linux side of the same contract.
///
/// `now_us` is the executor's monotonic clock, `None` on a build with no clock
/// at all. Then nothing is judged: a clock that never moves is a window that
/// never elapses, and guessing a window from spin counts would put a tolerance
/// nobody declared into a safety rule.
fn check_silence(spec: &AgeMonitorSpec, state: &mut AgeState, took: u32, now_us: u64) -> bool {
    let now_ms = (now_us / 1_000) as u32;
    if !state.silence_opened {
        state.silence_opened = true;
        state.last_take_ms = now_ms;
        return false;
    }
    if took > 0 {
        state.last_take_ms = now_ms;
        state.silence_reported = false;
        return false;
    }
    if now_ms.wrapping_sub(state.last_take_ms) < spec.max_age_ms {
        return false;
    }
    if state.silence_reported {
        return false; // still silent -- already reported
    }
    state.silence_reported = true;
    true
}

/// Pure age check: drains the sub cell's window-max take-age and fires
/// when it exceeds the declared bound. Report-once-until-recovery.
///
/// phase-462 W2 -- also the silence rule's tick, because the two verdicts read
/// ONE observation (what this endpoint took since the last check) and reading
/// it twice would let one rule's drain hide the other's evidence. They cannot
/// both fire: with no takes the drained age is 0, so `max-age-runtime` has
/// nothing to judge.
pub(crate) fn check_age(
    spec: &AgeMonitorSpec,
    state: &mut AgeState,
    now_us: Option<u64>,
) -> Option<Violation> {
    if spec.max_age_ms == 0 {
        return None;
    }
    let took = spec.cell.takes.swap(0, Ordering::Relaxed);
    if let Some(now_us) = now_us
        && check_silence(spec, state, took, now_us)
    {
        return Some(Violation {
            rule: "silence-runtime",
            fqn: spec.fqn,
            // Takes in the window, against the window the contract declares.
            measured: 0,
            declared: spec.max_age_ms,
        });
    }
    let max_ms = spec.cell.max_age_ms.swap(0, Ordering::Relaxed);
    if max_ms > spec.max_age_ms {
        if state.violated_last_window {
            return None;
        }
        state.violated_last_window = true;
        Some(Violation {
            rule: "max-age-runtime",
            fqn: spec.fqn,
            measured: max_ms,
            declared: spec.max_age_ms,
        })
    } else {
        state.violated_last_window = false;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CELL: PubMonitorCell = PubMonitorCell::new();

    /// phase-467 W1 -- the Autoware Safety Island's shape: 14 rate rows and
    /// no age rows fit an executor sized 14/0, and a 15th rate row is refused
    /// with the knob to raise NAMED, not truncated.
    #[test]
    fn a_table_past_the_knob_is_refused_and_names_it() {
        assert_eq!(check_capacity_against(14, 0, 14, 0), Ok(()));
        let full = check_capacity_against(15, 0, 14, 0).expect_err("15 > 14 must refuse");
        assert_eq!(full.knob, "NROS_EXECUTOR_MAX_MONITORS");
        assert_eq!((full.rows, full.capacity), (15, 14));
        let said = std::format!("{full}");
        assert!(
            said.contains("raise NROS_EXECUTOR_MAX_MONITORS"),
            "the refusal must name the knob: {said}"
        );
        let age = check_capacity_against(0, 1, 14, 0).expect_err("an age row past 0 must refuse");
        assert_eq!(age.knob, "NROS_EXECUTOR_MAX_AGE_MONITORS");
        // The two tables are separate knobs: a full age table does not borrow
        // from the rate one.
        assert!(check_capacity_against(3, 9, 14, 8).is_err());
    }

    /// The generated consts ARE the public ones, so the installers and the
    /// spin loop read one number.
    #[test]
    fn the_public_bounds_are_the_generated_config() {
        assert_eq!(MAX_MONITORS, crate::config::MAX_MONITORS);
        assert_eq!(MAX_AGE_MONITORS, crate::config::MAX_AGE_MONITORS);
        assert_eq!(check_table_capacity(MAX_MONITORS, MAX_AGE_MONITORS), Ok(()));
        assert!(check_table_capacity(MAX_MONITORS + 1, 0).is_err());
    }

    fn spec(min_milli: u32) -> MonitorSpec {
        MonitorSpec {
            topic: "/chatter",
            fqn: "/demo/talker/chatter",
            min_rate_hz_milli: min_milli,
            max_latency_ms: 0,
            cell: &CELL,
        }
    }

    #[test]
    fn slow_publisher_fires_once_until_recovery() {
        CELL.count.store(0, Ordering::Relaxed);
        let s = spec(100_000); // 100 Hz declared
        let mut st = MonitorState::default();

        // t=0: opens window.
        assert!(check_rate(&s, &mut st, 0).is_none());
        // 5 publishes in 5 s = 1 Hz — violation.
        CELL.count.store(5, Ordering::Relaxed);
        let v = check_rate(&s, &mut st, RATE_CHECK_INTERVAL_US).expect("fires");
        assert_eq!(v.rule, "rate-hierarchy-runtime");
        assert_eq!(v.fqn, "/demo/talker/chatter");
        assert_eq!(v.measured, 1_000);
        assert_eq!(v.declared, 100_000);
        // Still slow next window — suppressed (no re-report spam).
        CELL.count.store(10, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 2 * RATE_CHECK_INTERVAL_US).is_none());
        // Recovers (500 publishes in 5 s = 100 Hz) — clean window resets.
        CELL.count.store(510, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 3 * RATE_CHECK_INTERVAL_US).is_none());
        // Degrades again — fires again.
        CELL.count.store(511, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 4 * RATE_CHECK_INTERVAL_US).is_some());
    }

    #[test]
    fn compliant_and_uncontracted_stay_silent() {
        static C2: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            min_rate_hz_milli: 500, // 0.5 Hz
            max_latency_ms: 0,
            cell: &C2,
        };
        let mut st = MonitorState::default();
        assert!(check_rate(&s, &mut st, 0).is_none());
        C2.count.store(5, Ordering::Relaxed); // 1 Hz measured ≥ 0.5 Hz declared
        assert!(check_rate(&s, &mut st, RATE_CHECK_INTERVAL_US).is_none());

        // min_rate 0 = uncontracted: never fires, no state.
        let s0 = MonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            cell: &C2,
        };
        let mut st0 = MonitorState::default();
        assert!(check_rate(&s0, &mut st0, 10 * RATE_CHECK_INTERVAL_US).is_none());
    }

    #[test]
    fn stale_take_fires_age_once_until_recovery() {
        static SC: SubMonitorCell = SubMonitorCell::new();
        let s = AgeMonitorSpec {
            topic: "/scan",
            fqn: "/perc/detector/scan",
            max_age_ms: 100,
            cell: &SC,
        };
        let mut st = AgeState::default();

        // Fresh message: stamped 5 ms ago -- silent. Every tick here SEES a
        // take, so the silence rule never opens a window (phase-462 W2).
        SC.observe(1_000_000_000, 1_000_005_000);
        assert!(check_age(&s, &mut st, Some(0)).is_none());
        // Stale: 250 ms old — fires with the measured age.
        SC.observe(1_000_000_000, 1_000_250_000);
        let v = check_age(&s, &mut st, Some(1_000)).expect("fires");
        assert_eq!(v.rule, "max-age-runtime");
        assert_eq!(v.fqn, "/perc/detector/scan");
        assert_eq!(v.measured, 250);
        assert_eq!(v.declared, 100);
        // Still stale next window — suppressed.
        SC.observe(1_000_000_000, 1_000_300_000);
        assert!(check_age(&s, &mut st, Some(2_000)).is_none());
        // Recovers — clean window resets; stale again refires.
        SC.observe(1_000_000_000, 1_000_010_000);
        assert!(check_age(&s, &mut st, Some(3_000)).is_none());
        SC.observe(1_000_000_000, 1_000_999_000);
        assert!(check_age(&s, &mut st, Some(4_000)).is_some());
    }

    /// phase-462 W2 -- an input that stops coming is a violation of the same
    /// `max_age_ms` promise, and the age rule cannot see it.
    #[test]
    fn a_silent_subscription_fires_once_until_it_is_fed() {
        static SC2: SubMonitorCell = SubMonitorCell::new();
        let s = AgeMonitorSpec {
            topic: "/scan",
            fqn: "/perc/detector/scan",
            max_age_ms: 100,
            cell: &SC2,
        };
        let mut st = AgeState::default();

        // t=0 opens the lease; a first check is never a verdict.
        SC2.observe(1_000_000_000, 1_000_005_000);
        assert!(check_age(&s, &mut st, Some(0)).is_none());
        // 50 ms of nothing, inside the declared 100 ms window.
        assert!(check_age(&s, &mut st, Some(50_000)).is_none());
        // 120 ms of nothing: the lease is up.
        let v = check_age(&s, &mut st, Some(120_000)).expect("silence fires");
        assert_eq!(v.rule, "silence-runtime");
        assert_eq!(v.fqn, "/perc/detector/scan");
        assert_eq!(v.measured, 0, "nothing was taken");
        assert_eq!(v.declared, 100, "the window the contract declares");
        // Still silent -- one fault, not one per tick.
        assert!(check_age(&s, &mut st, Some(500_000)).is_none());
        // Fed again: the lease restarts and a later silence reports afresh.
        SC2.observe(1_000_000_000, 1_000_005_000);
        assert!(check_age(&s, &mut st, Some(600_000)).is_none());
        assert!(check_age(&s, &mut st, Some(650_000)).is_none());
        assert!(check_age(&s, &mut st, Some(800_000)).is_some());
    }

    /// The negative control for the same rule, in three parts: an
    /// uncontracted endpoint, a fed endpoint, and a build with no clock.
    #[test]
    fn silence_needs_a_contract_a_gap_and_a_clock() {
        static SC3: SubMonitorCell = SubMonitorCell::new();
        // No age contract: nothing is promised, so nothing is judged.
        let uncontracted = AgeMonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            max_age_ms: 0,
            cell: &SC3,
        };
        let mut st = AgeState::default();
        assert!(check_age(&uncontracted, &mut st, Some(0)).is_none());
        assert!(check_age(&uncontracted, &mut st, Some(10_000_000)).is_none());

        // Contracted and fed on every tick: silent rule, silent drain.
        let s = AgeMonitorSpec {
            max_age_ms: 100,
            ..uncontracted
        };
        let mut st = AgeState::default();
        for tick in 0..10u64 {
            SC3.observe(1_000_000_000, 1_000_001_000);
            assert!(
                check_age(&s, &mut st, Some(tick * 1_000_000)).is_none(),
                "a fed endpoint never reports silence"
            );
        }

        // No clock: degrade, never guess. A window measured in spins would be
        // a tolerance nobody declared.
        let mut st = AgeState::default();
        assert!(check_age(&s, &mut st, None).is_none());
        assert!(check_age(&s, &mut st, None).is_none());
    }

    #[test]
    fn peek_stamp_reads_le_time_and_rejects_unstamped() {
        // Encapsulation header (4B) + sec=100 nsec=5000 at offset 4.
        let mut raw = [0u8; 12];
        raw[4..8].copy_from_slice(&100i32.to_le_bytes());
        raw[8..12].copy_from_slice(&5_000u32.to_le_bytes());
        assert_eq!(peek_stamp_us(&raw, 4), Some(100_000_005));
        // Zero / negative sec = unstamped: no age sample.
        assert_eq!(peek_stamp_us(&[0u8; 12], 4), None);
        // Short buffer: no panic, no sample.
        assert_eq!(peek_stamp_us(&raw[..8], 4), None);
    }

    #[test]
    fn slow_path_fires_latency_once_until_recovery() {
        static C3: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/cmd",
            fqn: "/ctrl/control/cmd",
            min_rate_hz_milli: 0,
            max_latency_ms: 10,
            cell: &C3,
        };
        let mut st = MonitorState::default();
        // 4 ms dispatch — within budget.
        C3.max_latency_us.store(4_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        assert_eq!(C3.max_latency_us.load(Ordering::Relaxed), 0, "drained");
        // 25 ms dispatch — fires.
        C3.max_latency_us.store(25_000, Ordering::Relaxed);
        let v = check_latency(&s, &mut st).expect("fires");
        assert_eq!(v.rule, "max-latency-runtime");
        assert_eq!(v.measured, 25);
        assert_eq!(v.declared, 10);
        // Still slow — suppressed; recovery resets.
        C3.max_latency_us.store(30_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        C3.max_latency_us.store(1_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        C3.max_latency_us.store(30_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_some());
    }
}

/// Issue #514 — emit one violation to the log.
///
/// A log line is deliberately the floor rather than a `/diagnostics`
/// publication: it needs no publisher, no topic wiring, and no contract
/// on the reporting path itself, so it works on a bare RTOS image and
/// during boot. Publishing the same verdicts as `DiagnosticArray`
/// belongs on top of this, not instead of it.
///
/// Free function rather than an `Executor` method because every call
/// site sits inside a loop that already borrows the executor's spec
/// tables.
pub(crate) fn log_violation(v: &Violation) {
    nros_log::log_warn!(
        nros_log::get_logger("nros"),
        "contract violation: {} {} measured={} declared={}",
        v.rule,
        v.fqn,
        v.measured,
        v.declared
    );
}

/// phase-474 I1 -- the drain-and-report hook's line: the issue-0514 line plus
/// the verdict's sequence number and the counters, so a reader who sees line
/// `#12` after `#9` knows two were missed, and `dropped` says whether the ring
/// lost any before this drain.
fn log_drained_violation(seq: u32, total: u32, dropped: u32, v: &Violation) {
    nros_log::log_warn!(
        nros_log::get_logger("nros"),
        "contract violation #{}: {} {} measured={} declared={} (total={} dropped={})",
        seq,
        v.rule,
        v.fqn,
        v.measured,
        v.declared,
        total,
        dropped
    );
}

/// Issue #505 — periodic activations a timer dropped because its
/// executor was blocked past the period boundary.
///
/// This rule exists because `check_rate` cannot see an isolated stall:
/// it samples publish counts over a ~5 s window, so 20 missed
/// activations of a 100 Hz loop are a 0.4% rate deficit — under any
/// sane declared minimum, silence. (And under
/// [`TimerOverrunPolicy::CatchUp`](super::arena::TimerOverrunPolicy)
/// the replayed activations refill the window entirely, so the rate
/// rule reports a HEALTHY loop while the tier is stalling.) The
/// overrun counter is exact, needs no window, and does not depend on
/// clock resolution.
///
/// `overruns` is the timer's monotonic saturating counter;
/// `last_reported` is the value at the previous check, so the verdict
/// is on the delta. Returns a violation when more than `tolerated`
/// activations were dropped since the last check.
pub(crate) fn check_timer_overrun(
    overruns: u32,
    last_reported: &mut u32,
    tolerated: u32,
) -> Option<Violation> {
    let dropped = overruns.saturating_sub(*last_reported);
    *last_reported = overruns;
    if dropped <= tolerated {
        return None;
    }
    Some(Violation {
        rule: "timer-overrun-runtime",
        // Timer entries carry no name at this altitude; same stand-in
        // as `deadline-miss-runtime`.
        fqn: "timer",
        measured: dropped,
        declared: tolerated,
    })
}

/// Issue #515 — report a spin wake that arrived so late the cadence it
/// claims cannot have been met.
///
/// Like `check_timer_overrun` and unlike the rate/age/latency rules, this
/// needs no baked spec table. The bound is the spin period ITSELF: the
/// caller passes it to `spin_once` as the pacing quantum, it is what
/// `system.toml` declares as `spin_period_us`, and a wake later than a full
/// period past its predecessor means an activation's worth of cadence was
/// lost. That is a contract failure for any declared period, so there is
/// nothing further to declare.
///
/// The tolerance is one whole period rather than zero, and deliberately so.
/// Sub-period lateness is ordinary scheduling noise -- on the measured FVP
/// lane the executor is late on a large fraction of wakes while still
/// holding its rate -- and a rule that fired on each one would report a
/// healthy system as broken thousands of times a second. What is NOT
/// ordinary is being a full period late, because by then the wake that
/// should have happened in between never did.
///
/// `max_jitter_us` is the executor's high-water since the last check and
/// `last_reported` the value at the previous one, so the verdict is on the
/// delta -- the same shape as the overrun counter, and for the same reason:
/// a maximum that has not moved is not a new fault.
pub(crate) fn check_release_jitter(
    max_jitter_us: u64,
    last_reported: &mut u64,
    period_us: u64,
) -> Option<Violation> {
    if period_us == 0 || max_jitter_us <= *last_reported {
        return None;
    }
    *last_reported = max_jitter_us;
    if max_jitter_us < period_us {
        return None;
    }
    Some(Violation {
        rule: "release-jitter-runtime",
        // Same stand-in as the timer and deadline rules: the spin loop is
        // not an endpoint and carries no fqn at this altitude.
        fqn: "spin",
        measured: max_jitter_us.min(u32::MAX as u64) as u32,
        declared: period_us.min(u32::MAX as u64) as u32,
    })
}

/// Report a spin thread whose stack has come closer to its end than the
/// declared minimum.
///
/// Unlike every other rule here the bound CANNOT be derived from something
/// already declared, and that is worth stating rather than papering over.
/// `check_timer_overrun` and `check_release_jitter` both judge against a
/// period the caller already passes in; there is no equivalent for a stack.
/// The executor never sees `stack_bytes` -- it lives in the spawn attr and
/// goes no further -- and the total is not portably queryable either:
/// FreeRTOS exposes the high-water mark and not the size it was taken
/// against, so even a percentage cannot be computed. A minimum headroom is
/// therefore a real declaration, and `min_bytes == 0` means the caller has
/// not made one, which disables the rule.
///
/// Reports on the WORST case, not on each crossing: `worst_reported` holds
/// the lowest headroom already reported, so a stack hovering just under the
/// bound says so once and then only when it gets worse. The same delta
/// discipline as the overrun and jitter rules, inverted because for headroom
/// smaller is worse.
pub(crate) fn check_stack_headroom(
    unused_bytes: usize,
    min_bytes: usize,
    worst_reported: &mut usize,
) -> Option<Violation> {
    if min_bytes == 0 || unused_bytes >= min_bytes {
        return None;
    }
    // `usize::MAX` is the "nothing reported yet" sentinel: any real headroom
    // is below it, so the first breach always reports.
    if *worst_reported != usize::MAX && unused_bytes >= *worst_reported {
        return None;
    }
    *worst_reported = unused_bytes;
    Some(Violation {
        rule: "stack-headroom-runtime",
        // The spin thread is not an endpoint; same stand-in as the timer,
        // deadline and jitter rules.
        fqn: "stack",
        measured: unused_bytes.min(u32::MAX as usize) as u32,
        declared: min_bytes.min(u32::MAX as usize) as u32,
    })
}

#[cfg(test)]
mod stack_headroom_rule_tests {
    use super::*;

    /// No declared minimum means no claim to breach.
    #[test]
    fn a_zero_minimum_disables_the_rule() {
        let mut worst = usize::MAX;
        assert!(check_stack_headroom(8, 0, &mut worst).is_none());
    }

    #[test]
    fn headroom_at_the_bound_is_not_a_breach() {
        let mut worst = usize::MAX;
        assert!(check_stack_headroom(1024, 1024, &mut worst).is_none());
        assert!(check_stack_headroom(2048, 1024, &mut worst).is_none());
    }

    #[test]
    fn reports_the_first_breach_with_both_numbers() {
        let mut worst = usize::MAX;
        let v = check_stack_headroom(512, 1024, &mut worst).expect("under the bound reports");
        assert_eq!(v.rule, "stack-headroom-runtime");
        assert_eq!(v.measured, 512);
        assert_eq!(v.declared, 1024);
    }

    /// Smaller is worse for headroom, so the delta runs the other way.
    #[test]
    fn only_a_new_low_is_a_new_fault() {
        let mut worst = usize::MAX;
        assert!(check_stack_headroom(512, 1024, &mut worst).is_some());
        assert!(check_stack_headroom(512, 1024, &mut worst).is_none());
        assert!(check_stack_headroom(600, 1024, &mut worst).is_none());
        let v = check_stack_headroom(100, 1024, &mut worst).expect("a new low reports");
        assert_eq!(v.measured, 100);
    }
}

#[cfg(test)]
mod publish_stamp_tests {
    use super::*;

    /// CDR: 4-byte encapsulation header, then `Time { i32 sec; u32 nanosec }`
    /// little-endian, so `sec` sits at byte 4 — the layout `STAMP_OFFSET`
    /// encodes.
    fn cdr_with_stamp(sec: i32, nanosec: u32) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[4..8].copy_from_slice(&sec.to_le_bytes());
        b[8..12].copy_from_slice(&nanosec.to_le_bytes());
        b
    }

    #[test]
    fn records_the_age_of_what_was_published() {
        let cell = PubMonitorCell::new();
        let raw = cdr_with_stamp(10, 0); // stamped at 10_000_000 us
        observe_publish_stamp(&cell, &raw, 4, 10_500_000);
        assert_eq!(
            cell.last_publish_stamp_age_us.load(Ordering::Relaxed),
            500_000,
            "published data was half a second old"
        );
    }

    /// The signature of a node that RE-STAMPED: it publishes data whose
    /// stamp is now, so downstream age is single-hop, not end-to-end.
    #[test]
    fn a_restamping_node_shows_near_zero_age() {
        let cell = PubMonitorCell::new();
        let raw = cdr_with_stamp(10, 0);
        observe_publish_stamp(&cell, &raw, 4, 10_000_000);
        assert_eq!(cell.last_publish_stamp_age_us.load(Ordering::Relaxed), 0);
    }

    /// A state, not a window maximum: one stale message at startup must not
    /// pin the value for the life of the process.
    #[test]
    fn the_latest_publish_replaces_the_previous() {
        let cell = PubMonitorCell::new();
        observe_publish_stamp(&cell, &cdr_with_stamp(10, 0), 4, 12_000_000);
        assert_eq!(
            cell.last_publish_stamp_age_us.load(Ordering::Relaxed),
            2_000_000
        );
        observe_publish_stamp(&cell, &cdr_with_stamp(20, 0), 4, 20_100_000);
        assert_eq!(
            cell.last_publish_stamp_age_us.load(Ordering::Relaxed),
            100_000,
            "a fresh publish replaces the old age rather than maxing with it"
        );
    }

    /// An unset stamp is not an age of `now`. `peek_stamp_us` rejects
    /// `sec <= 0`, so a zeroed header records nothing at all.
    #[test]
    fn an_unstamped_message_records_nothing() {
        let cell = PubMonitorCell::new();
        observe_publish_stamp(&cell, &cdr_with_stamp(0, 0), 4, 5_000_000);
        assert_eq!(
            cell.last_publish_stamp_age_us.load(Ordering::Relaxed),
            0,
            "no stamp means no observation, not an enormous age"
        );
    }
}

/// Per-SchedContext liveness accounting, one per SC slot.
///
/// `Default` is DERIVED, not hand-written: every field's default is its type's
/// own (`false`, `0`), so a manual impl restates four of them and is one edit
/// away from disagreeing with the struct — `clippy::derivable_impls` is a
/// `-D warnings` error in the embedded lane, which is where this was caught.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AliveState {
    pub opened: bool,
    pub window_start_us: u64,
    pub count_at_window_start: u32,
    /// Report-once-until-recovery, matching the rate rule.
    pub violated_last_window: bool,
}

/// One SchedContext's alive-supervision record: the counter the dispatch
/// path bumps and the window state the monitor tick reads.
///
/// A region of the carved backing (`storage::carve`), one per SC the
/// executor was sized for — NOT an inline `[_; MAX_SC]` in the header.
/// `MAX_SC` is a sizing knob, so an inline table grows the `Executor` value
/// with it, which is the regression issue 0961 exists to prevent.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AliveSlot {
    pub dispatches: u32,
    pub state: AliveState,
}

/// Report a SchedContext that has not dispatched AT ALL over a full window
/// while declaring a period that says it should have.
///
/// AUTOSAR's Watchdog Manager separates ALIVE supervision -- did it run at
/// all, at roughly the right rate -- from DEADLINE supervision, did it finish
/// in time. Every rule here is the second kind or a variant of it: they fire
/// when something happens and is wrong. Nothing fires when a callback stops
/// happening altogether.
///
/// `rate-hierarchy-runtime` does not cover it. That rule counts PUBLISHES on
/// a contracted publisher, so a timer callback that silently stops firing
/// while its topic is published from elsewhere -- another tier, a bridge, a
/// republisher -- leaves it satisfied. The gap is a callback, not an
/// endpoint.
///
/// The bound needs no declaration: `period_us` is already on the SC, and a
/// context claiming a period that produced no activation in a window many
/// times longer than it is a contract failure for any period. `period_us ==
/// 0` declares no cadence, so nothing is judged.
///
/// ZERO, not a fraction. A throttled-but-alive context is a different fault
/// with different evidence -- `SporadicState` already counts window skips and
/// dispatches for exactly that -- and picking a percentage here would invent
/// a tolerance nobody declared. Zero activations against a declared period is
/// unambiguous.
pub(crate) fn check_alive(
    period_us: u64,
    dispatches: u32,
    state: &mut AliveState,
    now_us: u64,
) -> Option<Violation> {
    if period_us == 0 {
        return None;
    }
    if !state.opened {
        state.opened = true;
        state.window_start_us = now_us;
        state.count_at_window_start = dispatches;
        return None;
    }
    let window_us = now_us.saturating_sub(state.window_start_us);
    if window_us < RATE_CHECK_INTERVAL_US {
        return None;
    }
    let fired = dispatches.wrapping_sub(state.count_at_window_start);
    state.window_start_us = now_us;
    state.count_at_window_start = dispatches;

    // Only judge a window long enough to have contained an activation. A
    // period longer than the check interval is not late merely for not having
    // fired yet.
    if window_us < period_us {
        return None;
    }
    if fired > 0 {
        state.violated_last_window = false;
        return None;
    }
    if state.violated_last_window {
        return None; // still silent — already reported
    }
    state.violated_last_window = true;
    Some(Violation {
        rule: "alive-supervision-runtime",
        // SCs carry no name at this altitude; same stand-in as the deadline
        // rule, which reports against the SC too.
        fqn: "sched-context",
        measured: 0,
        declared: (window_us / period_us).min(u32::MAX as u64) as u32,
    })
}

#[cfg(test)]
mod alive_supervision_tests {
    use super::*;

    const W: u64 = RATE_CHECK_INTERVAL_US;

    #[test]
    fn a_zero_period_declares_no_cadence() {
        let mut st = AliveState::default();
        assert!(check_alive(0, 0, &mut st, 0).is_none());
        assert!(check_alive(0, 0, &mut st, W * 4).is_none());
    }

    /// The first observation opens the window; there is no verdict yet.
    #[test]
    fn the_first_window_only_opens() {
        let mut st = AliveState::default();
        assert!(check_alive(1_000, 0, &mut st, 0).is_none());
        assert!(st.opened);
    }

    #[test]
    fn silence_across_a_full_window_reports() {
        let mut st = AliveState::default();
        check_alive(1_000, 0, &mut st, 0);
        let v = check_alive(1_000, 0, &mut st, W).expect("no dispatch in a full window");
        assert_eq!(v.rule, "alive-supervision-runtime");
        assert_eq!(v.measured, 0);
        assert_eq!(v.declared, (W / 1_000) as u32);
    }

    /// Any activation at all clears it -- this rule asks whether the context
    /// is alive, not whether it is keeping up.
    #[test]
    fn a_single_dispatch_is_enough() {
        let mut st = AliveState::default();
        check_alive(1_000, 0, &mut st, 0);
        assert!(check_alive(1_000, 1, &mut st, W).is_none());
    }

    /// Report once, then stay quiet until it recovers, like the rate rule.
    #[test]
    fn continued_silence_is_not_a_new_fault() {
        let mut st = AliveState::default();
        check_alive(1_000, 0, &mut st, 0);
        assert!(check_alive(1_000, 0, &mut st, W).is_some());
        assert!(check_alive(1_000, 0, &mut st, W * 2).is_none());
        // Recovery, then silence again, reports afresh.
        assert!(check_alive(1_000, 5, &mut st, W * 3).is_none());
        assert!(check_alive(1_000, 5, &mut st, W * 4).is_some());
    }

    /// A period longer than the window has not had its chance yet.
    #[test]
    fn a_period_longer_than_the_window_is_not_judged() {
        let mut st = AliveState::default();
        check_alive(W * 10, 0, &mut st, 0);
        assert!(check_alive(W * 10, 0, &mut st, W).is_none());
    }
}

#[cfg(test)]
mod release_jitter_rule_tests {
    use super::*;

    /// Sub-period lateness is noise, not a violation -- otherwise a
    /// healthy-but-jittery loop reports thousands of faults a second.
    #[test]
    fn tolerates_lateness_within_one_period() {
        let mut last = 0;
        assert!(check_release_jitter(4_000, &mut last, 5_000).is_none());
        assert_eq!(last, 4_000, "still recorded, so the next delta is honest");
    }

    /// A full period late means the wake that belonged in between never
    /// happened.
    #[test]
    fn reports_a_wake_a_whole_period_late() {
        let mut last = 0;
        let v = check_release_jitter(5_000, &mut last, 5_000).expect("one period late reports");
        assert_eq!(v.rule, "release-jitter-runtime");
        assert_eq!(v.measured, 5_000);
        assert_eq!(v.declared, 5_000);
    }

    /// The verdict is on the DELTA. A high-water that has not moved is the
    /// same fault already reported, not a new one.
    #[test]
    fn an_unchanged_maximum_is_not_a_new_fault() {
        let mut last = 0;
        assert!(check_release_jitter(9_000, &mut last, 5_000).is_some());
        assert!(check_release_jitter(9_000, &mut last, 5_000).is_none());
        assert!(check_release_jitter(12_000, &mut last, 5_000).is_some());
    }

    /// No declared period means no cadence to be late for.
    #[test]
    fn a_zero_period_declares_nothing() {
        let mut last = 0;
        assert!(check_release_jitter(1_000_000, &mut last, 0).is_none());
    }
}

#[cfg(test)]
mod timer_overrun_rule_tests {
    use super::*;

    #[test]
    fn reports_the_delta_not_the_total() {
        let mut last = 0;
        let v = check_timer_overrun(19, &mut last, 0).expect("first drop reports");
        assert_eq!(v.rule, "timer-overrun-runtime");
        assert_eq!(v.measured, 19);
        // Same total on the next check is not a new fault.
        assert!(check_timer_overrun(19, &mut last, 0).is_none());
        // Only the newly dropped activations are reported.
        assert_eq!(check_timer_overrun(25, &mut last, 0).unwrap().measured, 6);
    }

    #[test]
    fn a_clean_timer_never_reports() {
        let mut last = 0;
        for _ in 0..10 {
            assert!(check_timer_overrun(0, &mut last, 0).is_none());
        }
    }

    #[test]
    fn tolerance_suppresses_small_drops() {
        let mut last = 0;
        assert!(check_timer_overrun(2, &mut last, 2).is_none());
        assert_eq!(check_timer_overrun(6, &mut last, 2).unwrap().measured, 4);
    }
}

// phase-436 E4 — when the stack-headroom query is due. The decision is pure so
// the three cases are pinned without a clock or a port.
#[cfg(test)]
mod stack_headroom_throttle_tests {
    use super::*;

    #[test]
    fn the_first_check_is_immediate_with_or_without_a_clock() {
        assert!(stack_headroom_check_due(Some(5), None, 0));
        assert!(stack_headroom_check_due(None, None, 0));
    }

    #[test]
    fn a_clocked_check_waits_one_full_interval() {
        let i = STACK_HEADROOM_CHECK_INTERVAL_US;
        assert!(!stack_headroom_check_due(
            Some(1_000 + i - 1),
            Some(1_000),
            0
        ));
        assert!(stack_headroom_check_due(Some(1_000 + i), Some(1_000), 0));
    }

    #[test]
    fn a_clockless_build_checks_once_per_stride_of_spins() {
        let s = STACK_HEADROOM_CHECK_SPIN_STRIDE;
        assert!(!stack_headroom_check_due(None, Some(0), s - 1));
        assert!(stack_headroom_check_due(None, Some(0), s));
    }
}

/// phase-474 I1 -- the wire vocabulary and the SWD record.
#[cfg(test)]
mod ring_record_tests {
    use super::*;

    fn v(rule: &'static str, fqn: &'static str, measured: u32) -> Violation {
        Violation {
            rule,
            fqn,
            measured,
            declared: 10,
        }
    }

    #[test]
    fn every_rule_the_executor_reports_has_a_stable_code() {
        // The order is the wire format: a capture or a RAM dump decodes by it.
        assert_eq!(rule_code("rate-hierarchy-runtime"), 1);
        assert_eq!(rule_code("max-latency-runtime"), 3);
        assert_eq!(rule_code("release-jitter-runtime"), 9);
        assert_eq!(rule_code("no-such-rule"), 0);
        for (i, r) in RULE_IDS.iter().enumerate() {
            assert_eq!(rule_name(rule_code(r)), Some(*r), "code {}", i + 1);
        }
        assert_eq!(rule_name(0), None);
        assert_eq!(rule_name(RULE_IDS.len() as u32 + 1), None);
    }

    #[test]
    fn fqn_hash_is_fnv1a_32() {
        // Reference values of 32-bit FNV-1a, so a Python decoder can match.
        assert_eq!(fqn_hash(""), 0x811c_9dc5);
        assert_eq!(fqn_hash("a"), 0xe40c_292c);
        assert_eq!(fqn_hash("foobar"), 0xbf9c_f968);
    }

    #[test]
    fn a_violation_is_four_marker_events() {
        let w = violation_marker_words(5, &v("max-latency-runtime", "/n/out", 25));
        assert_eq!(w[0], (MARKER_VIOLATION, (5 << 8) | 3));
        assert_eq!(w[1], (MARKER_VIOLATION_FQN, fqn_hash("/n/out")));
        assert_eq!(w[2], (MARKER_VIOLATION_MEASURED, 25));
        assert_eq!(w[3], (MARKER_VIOLATION_DECLARED, 10));
        // The sequence number keeps its low 24 bits; the rule its low 8.
        let w = violation_marker_words(0x0100_0007, &v("silence-runtime", "/x", 0));
        assert_eq!(w[0].1, (7 << 8) | 7);
    }

    #[test]
    fn the_record_keeps_the_latest_and_counts_what_it_overwrote() {
        let r: ViolationRecord<3> = ViolationRecord::new();
        assert_eq!(r.magic.load(Ordering::Relaxed), RECORD_MAGIC);
        assert_eq!(r.capacity.load(Ordering::Relaxed), 3);
        for m in 1..=5 {
            assert_eq!(r.store(&v("timer-overrun-runtime", "timer", m)), m);
        }
        assert_eq!(r.total.load(Ordering::Relaxed), 5);
        assert_eq!(r.dropped.load(Ordering::Relaxed), 2);
        assert_eq!(r.head.load(Ordering::Relaxed), 5 % 3);
        let mut seen = std::vec::Vec::new();
        r.newest_first(|seq, rule, fqn, measured, declared| {
            assert_eq!(rule, rule_code("timer-overrun-runtime"));
            assert_eq!(fqn, fqn_hash("timer"));
            assert_eq!(declared, 10);
            seen.push((seq, measured));
        });
        assert_eq!(seen, [(5, 5), (4, 4), (3, 3)], "newest first, oldest gone");
    }

    /// The layout a debugger decodes: `u32` words only, header then slots, the
    /// same size on every target. `scripts/read-violation-record.py` reads it
    /// by these numbers.
    #[test]
    fn the_record_is_packed_u32_words() {
        use core::mem::{offset_of, size_of};
        assert_eq!(size_of::<ViolationSlot>(), RECORD_SLOT_WORDS as usize * 4);
        assert_eq!(
            offset_of!(ViolationRecord<4>, slots),
            RECORD_HEADER_WORDS as usize * 4
        );
        assert_eq!(
            size_of::<ViolationRecord<4>>(),
            (RECORD_HEADER_WORDS + 4 * RECORD_SLOT_WORDS) as usize * 4
        );
        assert_eq!(offset_of!(ViolationRecord<4>, total), 16);
        assert_eq!(offset_of!(ViolationRecord<4>, head), 20);
        assert_eq!(offset_of!(ViolationRecord<4>, dropped), 24);
        assert_eq!(offset_of!(ViolationRecord<4>, suppressed_before_arm), 28);
        assert_eq!(offset_of!(ViolationRecord<4>, armed), 32);
        assert_eq!(offset_of!(ViolationSlot, fqn_addr), 20);
    }
}

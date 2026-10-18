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
//!   take → publish, measured on the executor's monotonic clock). The span
//!   is callback entry to exit, for every dispatch kind: a timer callback
//!   is timed whether it fires in the drains or in the trigger-miss sweep
//!   (phase-474 I6), and a service call it makes is inside the span. The
//!   timer's release jitter (a late start) is not part of it; that is
//!   `release-jitter-runtime`.
//! - `deadline-miss-runtime` — a dispatched callback ran past its bound
//!   SchedContext's `deadline_us`; what ELSE happens is the tier's
//!   [`DeadlineAction`](super::sched_context::DeadlineAction).
//!
//! phase10-B3 adds the route rule beside it:
//! - `route-latency-runtime` -- a node path whose trigger is an INPUT is
//!   budgeted from the TAKE of that input to its output, and on a node that
//!   reads its input on a timer tick that span includes the wait for the next
//!   tick. `max-latency-runtime` times one dispatch and cannot see the wait,
//!   so a route made of a long tick wait and a slow dispatch could exceed its
//!   budget while every callback passed. The executor stamps the dispatch that
//!   takes the trigger input (the earliest take not yet answered) and, at the
//!   end of a dispatch that published one of the path's outputs, judges
//!   take-to-dispatch-end against the path's `max_latency_ms`
//!   (`route_take`, `route_judge`). Record only: the rule has no action.
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
    /// phase10-B3 -- the route rule's state for this output
    /// ([`MonitorSpec::route_trigger`]): the trigger subscription's executor
    /// slot + 1 in the low 16 bits (0 = no subscription bound, set when the
    /// subscription registers), [`ROUTE_VIOLATED`] and [`ROUTE_PENDING`] above.
    pub route: AtomicU32,
    /// phase10-B3 -- when the earliest take not yet answered by a publish of
    /// this output happened, executor clock in microseconds (wrapping). Valid
    /// while [`ROUTE_PENDING`] is set in `route`.
    pub route_take_us: AtomicU32,
}

/// phase10-B3 -- `PubMonitorCell::route`: a take of the trigger input is
/// waiting for this output.
pub const ROUTE_PENDING: u32 = 1 << 31;
/// phase10-B3 -- `PubMonitorCell::route`: the last judged route was over its
/// budget (report once until one is within it, as the latency rule does).
pub const ROUTE_VIOLATED: u32 = 1 << 30;
/// phase10-B3 -- `PubMonitorCell::route`: the bound slot + 1.
pub const ROUTE_SLOT_MASK: u32 = 0xffff;

impl PubMonitorCell {
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            max_latency_us: AtomicU32::new(0),
            last_publish_stamp_age_us: AtomicU32::new(0),
            route: AtomicU32::new(0),
            route_take_us: AtomicU32::new(0),
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
    /// phase-462 W5b -- the executor slot + 1 whose readiness is this
    /// endpoint's arrival evidence, set when the contract's `deadline` is
    /// judged here rather than by the backend ([`admit_contract_deadline`]).
    /// 0 = not armed: the backend judges the deadline, or none is declared.
    pub deadline_slot: AtomicU32,
    /// phase-462 W5b -- spins on which the armed slot had data, drained by
    /// `check_age`. Counted for ANY message type, stamped or not, because a
    /// deadline is a promise about arrival and says nothing about stamps.
    pub arrivals: AtomicU32,
}

impl SubMonitorCell {
    pub const fn new() -> Self {
        Self {
            max_age_ms: AtomicU32::new(0),
            takes: AtomicU32::new(0),
            deadline_slot: AtomicU32::new(0),
            arrivals: AtomicU32::new(0),
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
    /// phase10-B3 -- the topic of the INPUT that triggers a node path whose
    /// output is this endpoint (`contracts.node_paths[..].input`, resolved to
    /// its wiring topic EXACTLY as the node passes it to
    /// `create_subscription`), or `""` for none. A subscription on this topic
    /// registered on the same executor is the route's trigger.
    pub route_trigger: &'static str,
    /// phase10-B3 -- that path's budget (ms), from the take of the trigger to
    /// this output (`route-latency-runtime`). 0 = no route contract.
    pub route_max_latency_ms: u32,
    /// phase-503 -- the route's SUPERVISED deadline (ms) from the take of the
    /// trigger: a take still unanswered at `take + route_deadline_ms` is
    /// reported (`route-deadline-runtime`) and the executor's reaction hook is
    /// called, without waiting for an output that may never come. 0 = the
    /// route is not supervised (only the completion verdict above judges it).
    /// Needs `route_trigger`; ignored without it.
    pub route_deadline_ms: u32,
    /// phase-503 -- the reaction the hook is asked to start when the deadline
    /// passes: an id codegen assigns to the contract's `reaction` path
    /// (1-based; [`REACTION_NONE`] = none named). Opaque to the executor.
    pub reaction_id: u16,
    /// phase-503 -- which mechanism the contract chose to supervise this
    /// route: [`MECHANISM_EXECUTOR`] (this check, in the spin loop) or
    /// [`MECHANISM_TIMER`] (an independent timer the port arms from
    /// `Executor::route_next_due_us`). The executor checks the deadline either
    /// way and passes the mechanism to the hook.
    pub mechanism: u8,
    /// The endpoint's counter cell.
    pub cell: &'static PubMonitorCell,
}

/// phase-503 -- [`MonitorSpec::reaction_id`]: no reaction named.
pub const REACTION_NONE: u16 = 0;
/// phase-503 -- [`MonitorSpec::mechanism`]: checked in the executor's spin
/// loop. Catches a slow route, not a callback that never returns.
pub const MECHANISM_EXECUTOR: u8 = 0;
/// phase-503 -- [`MonitorSpec::mechanism`]: an independent timer outside the
/// executor (the port's), which also catches a stuck callback.
pub const MECHANISM_TIMER: u8 = 1;

/// phase-503 -- what the reaction hook receives when a supervised route
/// passes its deadline. `repr(C)`: the C ABI hands the same struct through.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactionEvent {
    /// [`MonitorSpec::reaction_id`] of the row (or, for `hazard-detected`,
    /// [`HazardSpec::reaction_id`] of the hazard).
    pub reaction_id: u32,
    /// Index of the row in the installed monitor table (for
    /// `hazard-detected`, of the hazard in the installed hazard table).
    pub row: u32,
    /// [`rule_code`] of the verdict (`route-deadline-runtime`).
    pub rule_code: u32,
    /// Time from the take to the check, ms.
    pub measured_ms: u32,
    /// The deadline, ms.
    pub declared_ms: u32,
    /// [`MonitorSpec::mechanism`] of the row.
    pub mechanism: u8,
}

/// phase-503 -- the reaction hook: called on the spin thread, inside
/// `spin_once`, once per missed route deadline. It must not block; it starts
/// the fallback (or hands it to another thread) and returns. `ctx` is what
/// was registered with the hook.
pub type ReactionHookFn =
    unsafe extern "C" fn(event: *const ReactionEvent, ctx: *mut core::ffi::c_void);

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
    /// phase-462 W5b -- the contract's `deadline` for this endpoint, in the
    /// wire's milliseconds. 0 = none declared. Judged HERE only when the
    /// subscription's backend cannot judge it (see
    /// [`admit_contract_deadline`]); otherwise the backend's own deadline
    /// event is the detector and this row carries the number unread.
    pub deadline_ms: u32,
    /// The endpoint's age accumulator.
    pub cell: &'static SubMonitorCell,
}

/// phase-462 W5b -- what a subscription's creation does about its contract
/// `deadline`, decided from the backend's OWN answer
/// (`Session::supported_qos_policies`) at the moment it is created.
///
/// The served set is a run-time fact -- a C backend answers through the
/// vtable (issue 1329) -- so it is asked here rather than restated in a build
/// table that could disagree with it. Three outcomes:
///
/// * the backend honours `DEADLINE` and the profile carries the deadline: the
///   backend judges it, nothing is armed;
/// * the backend does not honour `DEADLINE`: a profile deadline EQUAL to the
///   contract's is stripped (it came from the contract, and refusing it at
///   create would turn a judgeable promise into a boot failure), and the row
///   is armed so the silence rule judges it;
/// * the profile never carried the deadline (a registration path that does
///   not fold the launch overrides): the backend cannot judge what it was
///   never told, so the row is armed too.
///
/// A profile deadline that DIFFERS from the contract's is the caller's own
/// request and is left alone, so a backend that cannot serve it still refuses
/// it by name. Returns the profile to create with and the row to arm.
pub fn admit_contract_deadline(
    mut qos: nros_rmw::QoSProfile,
    served: nros_rmw::QoSPolicyMask,
    ages: &'static [AgeMonitorSpec],
    topic: &str,
) -> (nros_rmw::QoSProfile, Option<&'static AgeMonitorSpec>) {
    let Some(spec) = ages.iter().find(|a| a.topic == topic && a.deadline_ms > 0) else {
        return (qos, None);
    };
    let serves = served.0 & nros_rmw::QoSPolicyMask::DEADLINE.0 != 0;
    if serves && qos.deadline_ms != 0 {
        return (qos, None);
    }
    if !serves && qos.deadline_ms == spec.deadline_ms {
        qos.deadline_ms = 0;
    }
    (qos, Some(spec))
}

/// phase-462 W5b -- a released subscription's slot stops being any row's
/// arrival evidence; its slot may be claimed by an unrelated entry next.
pub(crate) fn deadline_unbind(ages: &[AgeMonitorSpec], slot: usize) {
    for spec in ages {
        let _ = spec.cell.deadline_slot.compare_exchange(
            slot as u32 + 1,
            0,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }
}

/// phase-462 W5b -- credit one arrival to every armed row whose slot is in
/// `ready` (the executor's readiness bits for this spin).
///
/// Readiness, not dispatch, is the evidence, because it is the one point every
/// entry passes on the spin thread: an os-priority entry is dispatched on a
/// worker thread that holds no executor state, and a cooperative one only when
/// its trigger passes.
pub(crate) fn credit_arrivals(ages: &[AgeMonitorSpec], ready: u64) {
    for spec in ages {
        let slot = spec.cell.deadline_slot.load(Ordering::Relaxed);
        if slot != 0 && slot <= 64 && ready & (1u64 << (slot - 1)) != 0 {
            spec.cell.arrivals.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// phase-436 E4 — how often the stack-headroom rule may query the port, in µs.
///
/// On a painted Zephyr stack the query walks every UNUSED byte from the stack
/// base (`z_stack_space_get`), so querying on every spin put that scan inside
/// the control loop. The high-water mark only grows, so a throttled check sees
/// the same low mark and only sees it later. Since E5 the query is also capped
/// at the declared minimum (`stack_unused_bytes_upto`), so it reads at most
/// that many bytes.
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
/// phase-496 W1 (issue 1471) -- generated, not a literal: the
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

/// A monitor table longer than the executor can watch -- phase-496 W1.
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

/// phase-462 W4 -- a mode's variant table is not the base table's length,
/// refused by [`super::Executor::set_mode_tables`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeTablesMismatch {
    /// The mode id whose tables differ.
    pub mode: u16,
    /// `(the mode's, the base's)` rate/latency row count.
    pub monitors: (usize, usize),
    /// `(the mode's, the base's)` age row count.
    pub ages: (usize, usize),
    /// phase-462 W4c -- a deadline row naming a SchedContext this executor
    /// does not have.
    pub missing_sc: Option<u8>,
}

impl core::fmt::Display for ModeTablesMismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(sc) = self.missing_sc {
            return write!(
                f,
                "mode {} moves the deadline of sched context {sc}, which this executor does \
                 not have: install the mode tables after the sched contexts (re-run the bake)",
                self.mode
            );
        }
        write!(
            f,
            "mode {} bakes {} rate/latency and {} age rows where the base has {} and {}: a \
             mode is a variant of the base tables, row for row (re-run the bake)",
            self.mode, self.monitors.0, self.ages.0, self.monitors.1, self.ages.1
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
    /// `"silence-runtime"` (phase-462 W2) | `"route-latency-runtime"`
    /// (phase10-B3) | `"route-deadline-runtime"` (phase-503).
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
pub const RULE_IDS: [&str; 15] = [
    "rate-hierarchy-runtime",
    "max-age-runtime",
    "max-latency-runtime",
    "deadline-miss-runtime",
    "stack-headroom-runtime",
    "alive-supervision-runtime",
    "silence-runtime",
    "timer-overrun-runtime",
    "release-jitter-runtime",
    // phase10-B3.
    "route-latency-runtime",
    // phase-503.
    "route-deadline-runtime",
    // phase-462 W3b -- the hazard observer, in play_launch's spelling (no
    // `-runtime` suffix) so one log reader serves both sides of a bridge.
    "hazard-detected",
    "hazard-reaction",
    "hazard-recovered",
    // phase-462 W4c.
    "mode-switch-runtime",
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
/// | 7 | `suppressed_before_arm` | verdicts before the monitors armed, all executors (phase-474 I2) |
/// | 8 | `armed` | executors whose monitors armed (counted as each arms; at its first spin by default) (phase-474 I2) |
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

/// phase-474 I2 -- application arm requests, image-wide. Each call of
/// [`request_monitor_arming`] bumps it; an executor waiting for the
/// application arms when it sees a value other than the one it was opened
/// with, so a request made before an executor existed does not arm it.
static ARM_REQUESTS: AtomicU32 = AtomicU32::new(0);

/// phase-474 I2 -- the application says its start-up is over: arm the
/// contract monitors of every executor in the image that waits for this
/// (`NROS_MONITOR_ARM_ON_CALL`, or [`Executor::set_monitor_arming`]).
///
/// Callable from anywhere -- a node's callback, another thread -- because it
/// only bumps an atomic; each executor arms at its next spin. On an executor
/// that is already armed (the default: armed at the first spin) it does
/// nothing. C: `rclc_monitors_arm()`; C++: `nros::arm_monitors()`.
///
/// [`Executor::set_monitor_arming`]: super::Executor::set_monitor_arming
pub fn request_monitor_arming() {
    ARM_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

/// phase-462 W4 -- one contract MODE's variant of an executor's two monitor
/// tables: the base tables' rows in the base's order, sharing the base's
/// cells, with the mode's numbers.
///
/// Because the rows and cells are the base's, a publisher or subscription
/// keeps the cell it bound at creation, and a hazard's guard keeps the row
/// index it names; only the requirement each row is judged against changes.
/// The bake guarantees that shape (`nros_orchestration_ir::mode_variants`);
/// [`super::Executor::set_mode_tables`] refuses a table of another length.
#[derive(Debug, Clone, Copy)]
pub struct ModeTables {
    /// The rate/latency table in this mode.
    pub monitors: &'static [MonitorSpec],
    /// The age table in this mode.
    pub ages: &'static [AgeMonitorSpec],
    /// phase-462 W4c -- the SchedContext deadlines this mode moves. Every
    /// mode's table, the base's included, lists the SAME contexts, so entering
    /// any mode leaves every moved deadline at that mode's value.
    pub deadlines: &'static [ModeDeadline],
    /// phase-462 W4c -- how long ENTERING this mode may take, ms: its reaction
    /// path's `max_latency`. 0 = the contract states none.
    pub switch_budget_ms: u32,
    /// phase-462 W4c -- that reaction path's key, the `mode-switch-runtime`
    /// report's endpoint ref ("" when there is none).
    pub reaction: &'static str,
}

/// phase-462 W4c -- one SchedContext's deadline in one mode.
///
/// The deadline is the one tier fact a switch can apply: the executor reads
/// `SchedContext::deadline_us` each time it judges a dispatch. A priority is a
/// thread's and a reservation is armed at boot, so the bake lists a mode that
/// moves either instead of baking it.
///
/// `repr(C)`: the C ABI's `nros_cpp_mode_deadline_t` is the same two fields.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeDeadline {
    /// The SchedContext's id on this executor.
    pub sc: u8,
    /// The deadline in this mode, µs; 0 = none.
    pub deadline_us: u32,
}

/// phase-462 W4c -- the `mode-switch-runtime` verdict for entering `t`: how
/// long the request could have waited, against the mode's budget.
///
/// An executor enters a requested mode at its next spin, so a request
/// waited at most the gap since the previous spin's check -- the measured
/// value is that gap, an upper bound on the switch's own latency. `None` when
/// the mode states no budget, the gap is within it, or there is no clock.
pub(crate) fn check_mode_switch(t: &ModeTables, waited_ms: Option<u32>) -> Option<Violation> {
    let waited = waited_ms?;
    (t.switch_budget_ms != 0 && waited > t.switch_budget_ms).then_some(Violation {
        rule: "mode-switch-runtime",
        fqn: t.reaction,
        measured: waited,
        declared: t.switch_budget_ms,
    })
}

/// phase-462 W4 -- the mode the application asked for. A STATE, not an
/// event: an executor opened after the request enters it at its first spin.
static MODE_REQUEST: AtomicU32 = AtomicU32::new(0);

/// phase-462 W4 -- ask every executor to judge by mode `id`'s tables.
///
/// `id` is the baked mode id (`0` = the base contract, `1..` the contract's
/// modes in key order; the entry names each as a constant). Like
/// [`request_monitor_arming`] this only stores an atomic: each executor
/// swaps its own tables at its next spin, so the switch is bounded by the
/// slowest executor's spin period and never races a rule mid-judgement. An
/// id an executor has no table for leaves it on the tables it has, and it
/// says so once.
pub fn request_mode(id: u16) {
    MODE_REQUEST.store(u32::from(id), Ordering::Release);
}

/// The mode last asked for with [`request_mode`] (0 when never asked).
#[must_use]
pub fn requested_mode() -> u16 {
    MODE_REQUEST.load(Ordering::Acquire) as u16
}

/// The current arm-request count (for an executor to remember at open).
pub(crate) fn arm_requests() -> u32 {
    ARM_REQUESTS.load(Ordering::Acquire)
}

/// phase-474 I2 -- when an executor's contract monitors start judging.
///
/// A verdict reached before arming is COUNTED
/// ([`Executor::violations_suppressed_before_arm`], and the SWD record's
/// `suppressed_before_arm`) but not stored, logged, sunk or traced: start-up
/// -- registration, the first join, inputs that have not started yet -- looks
/// like overruns, jitter and silence, and judging it as steady state filled
/// the island's ring with eight start-up entries before any act ran.
///
/// The default is read from Kconfig / the build environment only
/// (`NROS_MONITOR_ARM_ON_CALL`, `NROS_MONITOR_ARM_GRACE_MS`); the contract
/// carries no key for it.
///
/// [`Executor::violations_suppressed_before_arm`]: super::Executor::violations_suppressed_before_arm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorArming {
    /// Wait for [`request_monitor_arming`] / `Executor::arm_monitors`.
    /// `false` (the default) arms at the first spin, today's behaviour.
    pub on_call: bool,
    /// Arm anyway this many ms after the first spin; 0 = no deadline. With
    /// `on_call` it is the backstop for an application that never calls; on
    /// its own it is a grace period for an application with no lifecycle.
    pub grace_ms: u32,
}

impl MonitorArming {
    /// The build's default (`NROS_MONITOR_ARM_ON_CALL`,
    /// `NROS_MONITOR_ARM_GRACE_MS`).
    pub const DEFAULT: Self = Self {
        on_call: crate::config::MONITOR_ARM_ON_CALL,
        grace_ms: crate::config::MONITOR_ARM_GRACE_MS,
    };

    /// Armed from the start: no wait for a call and no grace period.
    pub const fn armed_at_first_spin(self) -> bool {
        !self.on_call && self.grace_ms == 0
    }
}

/// Issue 1635 -- where an executor hands its violations when the image asked
/// it to (`Executor::set_violation_sink`).
///
/// A function and an opaque context rather than a closure, because the
/// executor is not generic and the reporter it reaches lives in a crate above
/// this one (`nros-cpp` publishes on `/diagnostics` through
/// `nros-diagnostics`). Called from inside the executor's spin (never from
/// inside a user callback) -- so the sink may publish.
///
/// phase-474 I7 -- called OFF the judged dispatch, not at detection: a stored
/// verdict is queued ([`REPORT_QUEUE`] deep) and the queue is handed over at
/// the end of a spin whose next timer is further away than the last report
/// cost (or once a verdict has waited [`REPORT_DEFER_MAX_US`]). The sink gets
/// every pending verdict, oldest first, and returns how many of them it took
/// -- a PREFIX, so it may coalesce as many as fit one message and leave the
/// rest for the next slot. Returning 0 is a failure the sink has counted;
/// the executor then drops the oldest pending verdict so a report that can
/// never be sent cannot stall the queue (it stays in the ring and the record).
///
/// On the safety island the report at detection cost 15-20 ms of spin per
/// verdict over a 921,600-baud serial link, inside the tick being judged, and
/// that cost tripped `release-jitter-runtime` and `timer-overrun-runtime`:
/// one verdict bred more.
///
/// # Safety
/// The sink is called with the `ctx` it was installed with; whoever installs it
/// guarantees `ctx` is valid for every call until the sink is replaced or the
/// executor is dropped.
pub type ViolationSink = unsafe fn(ctx: *mut core::ffi::c_void, pending: &[Violation]) -> usize;

/// phase-474 I7 -- how many stored verdicts wait for the sink. A verdict
/// stored while the queue is full evicts the oldest pending one, which is
/// counted (`ReportQueue::overflowed`); both stay in the ring and the SWD
/// record. Four is the `DiagnosticArray` status capacity of the generated
/// type, so one coalesced report never needs more.
pub const REPORT_QUEUE: usize = 4;

/// phase-474 I7 -- the longest a pending verdict waits for a slot with slack
/// before it is reported anyway, in microseconds. Bounds the deferral on an
/// executor whose timers never leave a gap as long as the report costs.
pub const REPORT_DEFER_MAX_US: u64 = 1_000_000;

/// phase-474 I7 -- verdicts stored but not yet handed to the sink, and what
/// the hand-over has cost. One struct so the executor size test can name it.
#[derive(Debug, Default)]
pub(crate) struct ReportQueue {
    /// Oldest first.
    pub(crate) pending: heapless::Vec<Violation, REPORT_QUEUE>,
    /// Monotonic us at which the oldest pending verdict was first seen by a
    /// flush, for [`REPORT_DEFER_MAX_US`].
    pub(crate) since_us: Option<u64>,
    /// What the last hand-over took, and the worst one, in us.
    pub(crate) last_cost_us: u32,
    pub(crate) max_cost_us: u32,
    /// Pending verdicts evicted by a newer one before any slot came (saturating).
    pub(crate) overflowed: u32,
    /// Verdicts the sink was handed and could not send (saturating).
    pub(crate) failed: u32,
    /// The reporter's own spin time since the last release-jitter sample: it
    /// is the monitor's cost, stated by `max_cost_us`, not lateness of the
    /// application's release, so the jitter rule does not charge it.
    pub(crate) debt_us: u64,
}

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
    /// Issue 1635 -- the image's reporter. The context is a `usize` so the
    /// executor keeps its auto traits; it is the installer's pointer.
    /// phase-474 I7 -- fed from `reports`, off the judged dispatch.
    pub(crate) sink: Option<(ViolationSink, usize)>,
    /// phase-474 I7 -- what waits for the sink.
    pub(crate) reports: ReportQueue,
    /// phase-409 -- CARVED, at `MAX_VIOLATIONS` (no `ExecutorSizing` knob:
    /// the build-time depth is the capability).
    pub(crate) ring: super::storage::CarvedVec<'s, Violation>,
    /// phase-474 I1/I2 -- the counters and the arming state.
    pub(crate) counts: ViolationCounts,
}

/// phase-474 I1/I2 -- the scalars of a [`ViolationChannel`]: its counters and
/// its arming state. One struct so `the_executor_value_does_not_scale_with_the_knobs`
/// can name what they cost; none of them scales with a knob.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViolationCounts {
    /// Violations this executor stored since boot (wrapping). It is also the
    /// sequence number of the newest entry in `ring`: every stored verdict is
    /// pushed, so the ring holds `total - len + 1 ..= total`.
    pub(crate) total: u32,
    /// Issue 0514 / phase-474 I1 -- entries evicted from the full ring before
    /// any drain took them (saturating).
    pub(crate) dropped: u32,
    /// phase-474 I2 -- whether verdicts are stored. Until then they are only
    /// counted in `suppressed_before_arm`.
    pub(crate) armed: bool,
    /// phase-474 I2 -- verdicts reached before arming (saturating).
    pub(crate) suppressed_before_arm: u32,
    /// phase-474 I2 -- the arming policy ([`MonitorArming`]), kept as its two
    /// fields so the flags pack beside the ones above.
    pub(crate) arm_on_call: bool,
    pub(crate) arm_grace_ms: u32,
    /// phase-474 I2 -- whether the executor has spun, and the monotonic ms
    /// (wrapping `u32`) of its first spin, which the grace deadline counts
    /// from.
    pub(crate) spun: bool,
    pub(crate) first_spin_ms: u32,
    /// phase-474 I2 -- [`arm_requests`] when the executor opened; a different
    /// value later is the application's call.
    pub(crate) arm_requests_seen: u32,
}

impl<'s> ViolationChannel<'s> {
    pub(crate) fn new(ring: super::storage::CarvedVec<'s, Violation>) -> Self {
        Self {
            report: true,
            drain_report: crate::config::VIOLATION_DRAIN_REPORT,
            sink: None,
            reports: ReportQueue::default(),
            ring,
            counts: ViolationCounts {
                total: 0,
                dropped: 0,
                armed: MonitorArming::DEFAULT.armed_at_first_spin(),
                suppressed_before_arm: 0,
                arm_on_call: MonitorArming::DEFAULT.on_call,
                arm_grace_ms: MonitorArming::DEFAULT.grace_ms,
                spun: false,
                first_spin_ms: 0,
                arm_requests_seen: arm_requests(),
            },
        }
    }

    /// Issue 1635 / phase-474 I1 -- THE one place a detected violation goes:
    /// the log floor (issue 0514) unless the drain hook reports instead, the
    /// image's sink's queue when one is installed (sent later, phase-474 I7),
    /// the trace marker when callback
    /// tracing is compiled in, the SWD record when the image keeps one, and
    /// the ring [`drain_violations`] reads.
    ///
    /// [`drain_violations`]: super::Executor::drain_violations
    pub(crate) fn record(&mut self, v: Violation) {
        if !self.counts.armed {
            // phase-474 I2 -- start-up: counted, never stored.
            self.counts.suppressed_before_arm = self.counts.suppressed_before_arm.saturating_add(1);
            #[cfg(nros_boot_report)]
            NROS_VIOLATION_RECORD
                .suppressed_before_arm
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        if self.report && !self.drain_report {
            log_violation(&v);
        }
        if self.sink.is_some() {
            // phase-474 I7 -- queued, not sent: the report leaves at the end of
            // a spin with slack (`Executor::flush_violation_reports`), so it
            // never costs the dispatch being judged.
            if self.reports.pending.is_full() {
                self.reports.pending.remove(0);
                self.reports.overflowed = self.reports.overflowed.saturating_add(1);
            }
            let _ = self.reports.pending.push(v.clone());
        }
        // `total` is this executor's sequence number for the verdict: the log
        // line and the trace marker carry it, so the two name one verdict by
        // one number. The SWD record numbers across every executor in the
        // image; on a single-executor image the two coincide.
        self.counts.total = self.counts.total.wrapping_add(1);
        #[cfg(nros_boot_report)]
        NROS_VIOLATION_RECORD.store(&v);
        #[cfg(feature = "trace-callbacks")]
        super::callback_trace::violation(self.counts.total, &v);
        if self.ring.capacity() == 0 {
            self.counts.dropped = self.counts.dropped.saturating_add(1);
            return;
        }
        if self.ring.len() == self.ring.capacity() {
            self.ring.pop_front();
            self.counts.dropped = self.counts.dropped.saturating_add(1);
        }
        // Cannot fail: a slot was just made if the ring was full.
        let _ = self.ring.push(v);
    }

    /// Hand every entry still in the ring to `f`, oldest first, with its
    /// sequence number, and empty the ring.
    pub(crate) fn drain(&mut self, mut f: impl FnMut(u32, &Violation)) {
        let n = self.ring.len() as u32;
        let first = self.counts.total.wrapping_sub(n).wrapping_add(1);
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
        let (total, dropped) = (self.counts.total, self.counts.dropped);
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
    /// phase-474 I8 -- the open window has been moved onto the stream's
    /// phase (or judged as opened). See [`check_rate`].
    pub(crate) aligned: bool,
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
        state.aligned = false;
        state.window_start_us = now_us;
        state.count_at_window_start = count;
        return None;
    }
    let window_us = now_us.saturating_sub(state.window_start_us);
    if !state.aligned && window_us < RATE_CHECK_INTERVAL_US {
        // phase-474 I8 -- the first window starts at the first tick that sees
        // a new sample, not where it was opened. The tick runs before the
        // spin's dispatch, so in steady state every boundary sits one tick
        // after a publish and the count matches the span. The window opened
        // at boot or by arming sits wherever that spin fell: off the
        // stream's phase it spans N samples over N periods plus the offset,
        // and a stream at exactly its declared rate read short (the island:
        // 9984 of 10000 mHz, once, after arming). Re-anchoring here puts the
        // first window on the same phase the later ones have. A stream that
        // stays silent is not held open: once a whole window passes with no
        // sample it is judged as opened (below), at 0.
        if count != state.count_at_window_start {
            state.aligned = true;
            state.window_start_us = now_us;
            state.count_at_window_start = count;
        }
        return None;
    }
    state.aligned = true;
    if window_us < RATE_CHECK_INTERVAL_US {
        return None;
    }
    let published = count.wrapping_sub(state.count_at_window_start) as u64;
    // milli-Hz = published * 1e3 / window_s = published * 1e9 / window_us
    let measured_milli_hz =
        (published.saturating_mul(1_000_000_000) / window_us.max(1)).min(u32::MAX as u64) as u32;
    // phase-474 I9 -- judge the count, not the floored quotient. The window
    // closes at the first tick that finds it elapsed, so `window_us` is the
    // declared interval plus that tick's lateness, while a stream at exactly
    // its declared minimum has published one sample per period: 50 over
    // 5.0005 s reads 9999 of 10000 mHz and was judged slow (the island, every
    // window, reported once thanks to `violated_last_window`). A stream at
    // the declared minimum delivers at least floor(min_rate * window) samples
    // in any window, whatever its phase; fewer than that is slow.
    let required = (spec.min_rate_hz_milli as u64).saturating_mul(window_us) / 1_000_000_000;

    // Roll the window.
    state.window_start_us = now_us;
    state.count_at_window_start = count;

    if published < required {
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

/// phase10-B3 -- bind a subscription slot as the route trigger of every row
/// whose [`MonitorSpec::route_trigger`] is `topic`. Called once per
/// registration; the first subscription on the topic keeps the binding.
pub(crate) fn route_bind(table: &[MonitorSpec], topic: &str, slot: usize) {
    if slot >= ROUTE_SLOT_MASK as usize {
        return;
    }
    for spec in table.iter().take(MAX_MONITORS) {
        if spec.route_max_latency_ms == 0 || spec.route_trigger != topic {
            continue;
        }
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_SLOT_MASK == 0 {
            spec.cell.route.store(
                (w & !ROUTE_SLOT_MASK) | (slot as u32 + 1),
                Ordering::Relaxed,
            );
        }
    }
}

/// phase10-B3 -- forget a released slot's route bindings and any take it left
/// waiting.
pub(crate) fn route_unbind(table: &[MonitorSpec], slot: usize) {
    for spec in table.iter().take(MAX_MONITORS) {
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_SLOT_MASK == slot as u32 + 1 {
            spec.cell.route.store(0, Ordering::Relaxed);
        }
    }
}

/// phase10-B3 -- the dispatch in `slot` took a sample at `take_us`: every
/// route whose trigger is that slot starts its clock, unless an earlier take
/// is still waiting for the output (the route is judged from the EARLIEST
/// unanswered take: a take that arrives just after a tick waits for the whole
/// next period, and that is the wait the budget has to hold).
pub(crate) fn route_take(table: &[MonitorSpec], slot: usize, take_us: u64) {
    let bound = slot as u32 + 1;
    for spec in table.iter().take(MAX_MONITORS) {
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_SLOT_MASK != bound || w & ROUTE_PENDING != 0 {
            continue;
        }
        spec.cell
            .route_take_us
            .store(take_us as u32, Ordering::Relaxed);
        spec.cell.route.store(w | ROUTE_PENDING, Ordering::Relaxed);
    }
}

/// phase10-B3 -- a dispatch ended at `end_us`. Every route output whose
/// counter advanced during it (`counts_before`, as the latency rule reads it)
/// and that has a take waiting is judged: take to dispatch end, an upper bound
/// on take to that publish, against the path's budget. Over budget is
/// reported once until a route is within it again.
///
/// phase-503 -- a SUPERVISED route ([`MonitorSpec::route_deadline_ms`])
/// answered at or after its deadline is also overdue: it is reported as
/// `route-deadline-runtime` and its reaction requested, as `route_overdue`
/// would have done had it run in time. Under `MECHANISM_EXECUTOR` this is the
/// only detection a single long callback gets: the callback that holds the
/// spin thread past the deadline publishes the output before the executor's
/// overdue check can run, so the miss is caught here, at completion -- late
/// by the callback's overrun. A route whose deadline was already caught
/// (`route_overdue` dropped its take) is not pending any more and is not
/// judged again, so each take gets at most one deadline verdict.
///
/// `report(row, violation, record, react)`: `record` is `false` while the
/// route is still marked violated from an earlier verdict (report once until
/// a route is within its budget again); `react` asks the caller to call the
/// reaction hook, on every late supervised take as `route_overdue` does.
pub(crate) fn route_judge(
    table: &[MonitorSpec],
    counts_before: &[u32; MAX_MONITORS],
    end_us: u64,
    mut report: impl FnMut(usize, Violation, bool, bool),
) {
    for (k, spec) in table.iter().take(MAX_MONITORS).enumerate() {
        if spec.route_max_latency_ms == 0 {
            continue;
        }
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_PENDING == 0 || spec.cell.count.load(Ordering::Relaxed) == counts_before[k] {
            continue;
        }
        let took_us = (end_us as u32).wrapping_sub(spec.cell.route_take_us.load(Ordering::Relaxed));
        let measured_ms = took_us / 1_000;
        let first = w & ROUTE_VIOLATED == 0;
        let late = measured_ms > spec.route_max_latency_ms;
        let overdue =
            route_supervised(spec) && took_us >= spec.route_deadline_ms.saturating_mul(1_000);
        let mut next = w & !ROUTE_PENDING;
        if late && first {
            report(
                k,
                Violation {
                    rule: "route-latency-runtime",
                    fqn: spec.fqn,
                    measured: measured_ms,
                    declared: spec.route_max_latency_ms,
                },
                true,
                false,
            );
        }
        if overdue {
            report(
                k,
                Violation {
                    rule: "route-deadline-runtime",
                    fqn: spec.fqn,
                    measured: measured_ms,
                    declared: spec.route_deadline_ms,
                },
                first,
                true,
            );
        }
        if late || overdue {
            next |= ROUTE_VIOLATED;
        } else {
            next &= !ROUTE_VIOLATED;
        }
        spec.cell.route.store(next, Ordering::Relaxed);
    }
}

/// phase-503 -- whether a pending route of `spec` has a supervised deadline.
#[inline]
fn route_supervised(spec: &MonitorSpec) -> bool {
    spec.route_deadline_ms > 0 && spec.route_max_latency_ms > 0
}

/// phase-503 -- time from now to the earliest deadline of a pending supervised
/// route, in microseconds (0 when one is already due), or `None` when no
/// supervised route is waiting. This is the executor's wake source for the
/// rule, and what a port's independent timer (`MECHANISM_TIMER`) arms on.
///
/// The due time is `take + route_deadline_ms`, derived from the stored take
/// rather than kept in a second cell: both are the clock's low 32 bits, so the
/// difference is exact across a wrap as long as a route waits less than ~71
/// minutes, and a supervised one is reported long before that.
pub fn route_next_due_us(table: &[MonitorSpec], now_us: u64) -> Option<u64> {
    let mut best: Option<u64> = None;
    for spec in table.iter().take(MAX_MONITORS) {
        if !route_supervised(spec) {
            continue;
        }
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_PENDING == 0 {
            continue;
        }
        let waited_us =
            (now_us as u32).wrapping_sub(spec.cell.route_take_us.load(Ordering::Relaxed));
        let deadline_us = spec.route_deadline_ms.saturating_mul(1_000);
        let left = deadline_us.saturating_sub(waited_us) as u64;
        best = Some(best.map_or(left, |b| b.min(left)));
        if left == 0 {
            break;
        }
    }
    best
}

/// phase-503 -- judge every pending supervised route at `now_us`: a take still
/// unanswered at or after `take + route_deadline_ms` is OVERDUE. Its waiting
/// take is dropped (the route stops pending, so the next take starts a new
/// one) and the route is marked [`ROUTE_VIOLATED`], as `route_judge` marks an
/// over-budget route.
///
/// `react(row, violation, first)` is called for every overdue route: `first`
/// is `false` while the route is still marked violated from an earlier verdict
/// (report once until a route is within its budget again, the latency rule's
/// rule), and the caller records the violation only when it is `true`. The
/// reaction is requested every time, since each overdue take is a missed
/// deadline the fallback has to cover.
pub(crate) fn route_overdue(
    table: &[MonitorSpec],
    now_us: u64,
    mut react: impl FnMut(usize, Violation, bool),
) {
    for (k, spec) in table.iter().take(MAX_MONITORS).enumerate() {
        if !route_supervised(spec) {
            continue;
        }
        let w = spec.cell.route.load(Ordering::Relaxed);
        if w & ROUTE_PENDING == 0 {
            continue;
        }
        let waited_us =
            (now_us as u32).wrapping_sub(spec.cell.route_take_us.load(Ordering::Relaxed));
        if waited_us < spec.route_deadline_ms.saturating_mul(1_000) {
            continue;
        }
        spec.cell
            .route
            .store((w & !ROUTE_PENDING) | ROUTE_VIOLATED, Ordering::Relaxed);
        react(
            k,
            Violation {
                rule: "route-deadline-runtime",
                fqn: spec.fqn,
                measured: waited_us / 1_000,
                declared: spec.route_deadline_ms,
            },
            w & ROUTE_VIOLATED == 0,
        );
    }
}

/// phase-503 -- whether any row carries a supervised route deadline.
pub(crate) fn route_deadline_active(table: &[MonitorSpec]) -> bool {
    table.iter().any(route_supervised)
}

/// phase10-B3 -- whether any row carries a route contract.
pub(crate) fn route_active(table: &[MonitorSpec]) -> bool {
    table.iter().any(|m| m.route_max_latency_ms > 0)
}

/// phase10-B3 -- drop every waiting take and the report-once state (arming:
/// a take from start-up is not a route of RUN's).
pub(crate) fn route_reset(table: &[MonitorSpec]) {
    for spec in table.iter() {
        let w = spec.cell.route.load(Ordering::Relaxed);
        spec.cell
            .route
            .store(w & ROUTE_SLOT_MASK, Ordering::Relaxed);
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
///
/// phase-462 W5b -- `window_ms` is [`silence_window`]'s: the age bound, or the
/// contract's `deadline` when the row is armed for one and it is shorter.
fn check_silence(window_ms: u32, state: &mut AgeState, took: u32, now_us: u64) -> bool {
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
    if now_ms.wrapping_sub(state.last_take_ms) < window_ms {
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
/// phase-462 W5b -- the silence rule's window and its evidence for one row:
/// `(window_ms, evidence)`, or `None` when the row judges no silence at all.
///
/// An age contract keeps W2's evidence (stamped takes, so an endpoint whose
/// type carries no stamp still reads as unjudgeable) and an armed deadline
/// shortens the window when it is the tighter bound. A row armed for a
/// deadline ALONE counts arrivals, since a deadline promises arrival and
/// nothing about stamps. Both counters are drained either way, so neither
/// carries a stale window into the next check.
fn silence_window(spec: &AgeMonitorSpec) -> Option<(u32, u32)> {
    let took = spec.cell.takes.swap(0, Ordering::Relaxed);
    let arrived = spec.cell.arrivals.swap(0, Ordering::Relaxed);
    let armed = spec.deadline_ms > 0 && spec.cell.deadline_slot.load(Ordering::Relaxed) != 0;
    match (spec.max_age_ms, armed) {
        (0, false) => None,
        (0, true) => Some((spec.deadline_ms, arrived)),
        (age, false) => Some((age, took)),
        (age, true) => Some((age.min(spec.deadline_ms), took)),
    }
}

pub(crate) fn check_age(
    spec: &AgeMonitorSpec,
    state: &mut AgeState,
    now_us: Option<u64>,
) -> Option<Violation> {
    let (window_ms, took) = silence_window(spec)?;
    if let Some(now_us) = now_us
        && check_silence(window_ms, state, took, now_us)
    {
        return Some(Violation {
            rule: "silence-runtime",
            fqn: spec.fqn,
            // Takes in the window, against the window the contract declares.
            measured: 0,
            declared: window_ms,
        });
    }
    if spec.max_age_ms == 0 {
        return None;
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

// ---------------------------------------------------------------------------
// phase-462 W3b -- the hazard observer.
// ---------------------------------------------------------------------------

/// One guard member: which monitor row's verdict says this topic has faulted.
///
/// The observer adds no detector of its own. A guard topic has faulted when
/// the row the image already bakes for it says so: an age row whose
/// `silence-runtime` lease is reported, or a rate row whose last window was
/// short (`min-rate-runtime`). Reading the rows' state rather than their
/// violations keeps the two in step by construction -- a hazard cannot be
/// detected by a rule that did not fire, nor stay detected after it recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardRef {
    /// Row index into the executor's age table.
    Age(u16),
    /// Row index into the executor's rate/latency table.
    Rate(u16),
}

/// One fault a hazard covers: a topic, or a redundant set of topics.
#[derive(Debug, Clone, Copy)]
pub struct GuardSpec {
    /// `true`: the guard faults only when EVERY member has (a redundant
    /// set). `false`: when any member has.
    pub all_of: bool,
    /// The guard's members.
    pub members: &'static [GuardRef],
}

/// [`HazardSpec::sink`] when the hazard's reaction states no `safe_state`
/// publisher this image monitors: the observer reports detection and
/// recovery, and no reaction.
pub const NO_SINK: u16 = u16::MAX;

/// One hazard's observer state. Baked as a `static` beside the spec, the way
/// [`PubMonitorCell`] is, so the observer adds a slice to the `Executor`
/// value and nothing per hazard (issue 0961).
#[derive(Debug, Default)]
pub struct HazardCell {
    /// 0 clear, 1 detected, 2 detected and reacted.
    state: AtomicU32,
    /// Monotonic ms the fault began: the faulting age row's last take, or the
    /// tick that first saw a rate guard short.
    fault_ms: AtomicU32,
    /// The sink publisher's count when the fault was detected.
    sink_count: AtomicU32,
}

impl HazardCell {
    /// A cleared cell, for a `static`.
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
            fault_ms: AtomicU32::new(0),
            sink_count: AtomicU32::new(0),
        }
    }
}

/// One contracted hazard (`contracts.hazards`), lowered by the bake.
#[derive(Debug, Clone, Copy)]
pub struct HazardSpec {
    /// The hazard's contract key (`<scope>/<name>`), the report key.
    pub key: &'static str,
    /// Declared `ftti_ms`; 0 = none declared.
    pub ftti_ms: u32,
    /// The reacting path's `safe_state.settle_ms`, added to the reaction
    /// time as play_launch adds it; 0 = none.
    pub settle_ms: u32,
    /// The faults the hazard covers; any one of them is the hazard.
    pub guards: &'static [GuardSpec],
    /// Rate-table row of the `safe_state.emits` publisher, or [`NO_SINK`].
    pub sink: u16,
    /// phase-462 W3c -- the id the reaction hook receives when this hazard is
    /// DETECTED (the reacting path's, in the model's one reaction id space
    /// shared with supervised routes); 0 = no safe-state entry to start.
    pub reaction_id: u16,
    /// The observer's state.
    pub cell: &'static HazardCell,
}

const HAZARD_CLEAR: u32 = 0;
const HAZARD_DETECTED: u32 = 1;
const HAZARD_REACTED: u32 = 2;

/// Whether `member` has faulted, and since when (ms) when the row knows.
fn member_fault(
    member: GuardRef,
    monitor_states: &[MonitorState],
    age_states: &[AgeState],
) -> Option<Option<u32>> {
    match member {
        GuardRef::Age(i) => {
            let s = age_states.get(i as usize)?;
            s.silence_reported.then_some(Some(s.last_take_ms))
        }
        GuardRef::Rate(i) => {
            let s = monitor_states.get(i as usize)?;
            s.violated_last_window.then_some(None)
        }
    }
}

/// Whether any guard of `spec` has faulted, and the earliest fault start the
/// faulting rows know.
fn hazard_fault(
    spec: &HazardSpec,
    monitor_states: &[MonitorState],
    age_states: &[AgeState],
) -> Option<Option<u32>> {
    let mut faulted = false;
    let mut since: Option<u32> = None;
    for g in spec.guards {
        let mut seen = 0usize;
        let mut g_since: Option<u32> = None;
        for m in g.members {
            if let Some(at) = member_fault(*m, monitor_states, age_states) {
                seen += 1;
                if let Some(at) = at {
                    g_since = Some(g_since.map_or(at, |s: u32| s.min(at)));
                }
            }
        }
        let guard_faulted = if g.all_of {
            !g.members.is_empty() && seen == g.members.len()
        } else {
            seen > 0
        };
        if guard_faulted {
            faulted = true;
            if let Some(at) = g_since {
                since = Some(since.map_or(at, |s| s.min(at)));
            }
        }
    }
    faulted.then_some(since)
}

/// phase-462 W3b -- one tick of a hazard's observer, after the monitor rows
/// have run. Emits, in play_launch's vocabulary so one log reader serves both
/// sides of a bridge:
///
/// - `hazard-detected` when a guard first faults: measured is the ms since
///   the fault began, declared the FTTI;
/// - `hazard-reaction` when the `safe_state` publisher publishes after that:
///   measured is fault-to-reaction plus `settle_ms`, declared the FTTI;
/// - `hazard-recovered` when no guard is faulted any more: measured is the
///   ms the hazard lasted.
///
/// `fqn` is the hazard's key on all three.
pub(crate) fn check_hazard(
    spec: &HazardSpec,
    monitor_table: &[MonitorSpec],
    monitor_states: &[MonitorState],
    age_states: &[AgeState],
    now_ms: u32,
    mut emit: impl FnMut(Violation),
) {
    let cell = spec.cell;
    let state = cell.state.load(Ordering::Relaxed);
    let sink_count = monitor_table
        .get(spec.sink as usize)
        .map(|m| m.cell.count.load(Ordering::Relaxed));
    let fault = hazard_fault(spec, monitor_states, age_states);
    let v = |rule, measured: u32| Violation {
        rule,
        fqn: spec.key,
        measured,
        declared: spec.ftti_ms,
    };
    match (state, fault) {
        (HAZARD_CLEAR, Some(since)) => {
            let at = since.unwrap_or(now_ms);
            cell.fault_ms.store(at, Ordering::Relaxed);
            cell.sink_count
                .store(sink_count.unwrap_or(0), Ordering::Relaxed);
            cell.state.store(HAZARD_DETECTED, Ordering::Relaxed);
            emit(v("hazard-detected", now_ms.wrapping_sub(at)));
        }
        (HAZARD_DETECTED, Some(_)) => {
            if let Some(count) = sink_count
                && count != cell.sink_count.load(Ordering::Relaxed)
            {
                cell.state.store(HAZARD_REACTED, Ordering::Relaxed);
                let since = now_ms.wrapping_sub(cell.fault_ms.load(Ordering::Relaxed));
                emit(v("hazard-reaction", since.saturating_add(spec.settle_ms)));
            }
        }
        (HAZARD_DETECTED | HAZARD_REACTED, None) => {
            cell.state.store(HAZARD_CLEAR, Ordering::Relaxed);
            let lasted = now_ms.wrapping_sub(cell.fault_ms.load(Ordering::Relaxed));
            emit(v("hazard-recovered", lasted));
        }
        _ => {}
    }
}

#[cfg(test)]
mod hazard_tests {
    use super::*;

    static PUB_CELL: PubMonitorCell = PubMonitorCell::new();
    static SUB_CELL: SubMonitorCell = SubMonitorCell::new();
    static SINK_CELL: PubMonitorCell = PubMonitorCell::new();

    static RATE_ROWS: [MonitorSpec; 2] = [
        MonitorSpec {
            topic: "/scan",
            fqn: "/sensor/scan",
            min_rate_hz_milli: 10_000,
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &PUB_CELL,
        },
        MonitorSpec {
            topic: "/stop",
            fqn: "/handler/stop",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &SINK_CELL,
        },
    ];
    #[allow(dead_code)]
    static AGE_ROW: AgeMonitorSpec = AgeMonitorSpec {
        topic: "/scan",
        fqn: "/handler/scan",
        max_age_ms: 100,
        deadline_ms: 0,
        cell: &SUB_CELL,
    };

    fn run(
        spec: &HazardSpec,
        mon: &[MonitorState],
        age: &[AgeState],
        now: u32,
    ) -> std::vec::Vec<Violation> {
        let mut out = std::vec::Vec::new();
        check_hazard(spec, &RATE_ROWS, mon, age, now, |v| out.push(v));
        out
    }

    /// detected -> reaction -> recovered, each once, on a silence guard, with
    /// times measured from the guard's last take.
    #[test]
    fn a_silence_guard_detects_reacts_and_recovers_once_each() {
        static CELL: HazardCell = HazardCell::new();
        static GUARDS: [GuardSpec; 1] = [GuardSpec {
            all_of: false,
            members: &[GuardRef::Age(0)],
        }];
        let spec = HazardSpec {
            key: "demo.launch.xml/scan_lost",
            ftti_ms: 500,
            settle_ms: 30,
            guards: &GUARDS,
            sink: 1,
            reaction_id: 0,
            cell: &CELL,
        };
        let mon = [MonitorState::default(); 2];
        let mut age = [AgeState::default(); 1];
        assert!(run(&spec, &mon, &age, 1_000).is_empty(), "clear: nothing");

        age[0].silence_reported = true;
        age[0].last_take_ms = 900;
        let v = run(&spec, &mon, &age, 1_000);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].rule, v[0].fqn), ("hazard-detected", spec.key));
        assert_eq!((v[0].measured, v[0].declared), (100, 500));
        assert!(run(&spec, &mon, &age, 1_010).is_empty(), "detected once");

        SINK_CELL.count.fetch_add(1, Ordering::Relaxed);
        let v = run(&spec, &mon, &age, 1_050);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "hazard-reaction");
        assert_eq!(v[0].measured, 150 + 30, "fault-to-reaction plus settle");
        SINK_CELL.count.fetch_add(1, Ordering::Relaxed);
        assert!(run(&spec, &mon, &age, 1_060).is_empty(), "reacted once");

        age[0].silence_reported = false;
        let v = run(&spec, &mon, &age, 1_200);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].rule, v[0].measured), ("hazard-recovered", 300));
        assert!(run(&spec, &mon, &age, 1_210).is_empty(), "recovered once");
    }

    /// A redundant set faults only when every member has.
    #[test]
    fn an_all_of_guard_waits_for_every_member() {
        static CELL: HazardCell = HazardCell::new();
        static GUARDS: [GuardSpec; 1] = [GuardSpec {
            all_of: true,
            members: &[GuardRef::Age(0), GuardRef::Rate(0)],
        }];
        let spec = HazardSpec {
            key: "demo.launch.xml/both_lost",
            ftti_ms: 0,
            settle_ms: 0,
            guards: &GUARDS,
            sink: NO_SINK,
            reaction_id: 0,
            cell: &CELL,
        };
        let mut mon = [MonitorState::default(); 2];
        let mut age = [AgeState::default(); 1];
        age[0].silence_reported = true;
        assert!(
            run(&spec, &mon, &age, 10).is_empty(),
            "one of two: no hazard"
        );
        mon[0].violated_last_window = true;
        let v = run(&spec, &mon, &age, 20);
        assert_eq!(v[0].rule, "hazard-detected");
        assert_eq!(v[0].declared, 0, "no FTTI declared");
    }

    /// phase-462 W4 -- the text `mode_variants::render_mode_tables_rs` emits
    /// after the base tables for a one-row model with one mode (its
    /// `the_variant_tables_render_over_the_base_cells` fixture), verbatim but
    /// for the crate root, so the generated shape compiles against the real
    /// types. phase-462 W4c -- with the deadline rows and switch budget of its
    /// `the_variant_tables_render_deadlines_and_the_switch_budget` fixture.
    #[allow(dead_code, non_upper_case_globals)]
    mod rendered_modes {
        use crate::executor::monitor::{AgeMonitorSpec, MonitorSpec, PubMonitorCell};

        static NROS_MONITOR_CELL_0: PubMonitorCell = PubMonitorCell::new();
        pub static NROS_MONITORS: &[MonitorSpec] = &[MonitorSpec {
            topic: "/cmd",
            fqn: "/talker/cmd",
            min_rate_hz_milli: 10000u32,
            max_latency_ms: 20u32,
            route_trigger: "",
            route_max_latency_ms: 0u32,
            route_deadline_ms: 0u32,
            reaction_id: 0u16,
            mechanism: 0u8,
            cell: &NROS_MONITOR_CELL_0,
        }];
        pub static NROS_AGE_MONITORS: &[AgeMonitorSpec] = &[];

        // phase-462 W4 -- the contract's modes: the base rows, the base cells, the
        // mode's numbers.
        pub static NROS_MODE_MONITORS_1: &[MonitorSpec] = &[MonitorSpec {
            topic: "/cmd",
            fqn: "/talker/cmd",
            min_rate_hz_milli: 5000u32,
            max_latency_ms: 20u32,
            route_trigger: "",
            route_max_latency_ms: 0u32,
            route_deadline_ms: 0u32,
            reaction_id: 0u16,
            mechanism: 0u8,
            cell: &NROS_MONITOR_CELL_0,
        }];
        pub static NROS_MODE_AGE_MONITORS_1: &[AgeMonitorSpec] = &[];
        pub static NROS_MODE_DEADLINES_0: &[crate::executor::monitor::ModeDeadline] =
            &[crate::executor::monitor::ModeDeadline {
                sc: 0u8,
                deadline_us: 5000u32,
            }];
        pub static NROS_MODE_DEADLINES_1: &[crate::executor::monitor::ModeDeadline] =
            &[crate::executor::monitor::ModeDeadline {
                sc: 0u8,
                deadline_us: 2000u32,
            }];
        pub static NROS_MODE_TABLES: &[crate::executor::monitor::ModeTables] = &[
            crate::executor::monitor::ModeTables {
                monitors: NROS_MONITORS,
                ages: NROS_AGE_MONITORS,
                deadlines: NROS_MODE_DEADLINES_0,
                switch_budget_ms: 0u32,
                reaction: "",
            },
            crate::executor::monitor::ModeTables {
                monitors: NROS_MODE_MONITORS_1,
                ages: NROS_MODE_AGE_MONITORS_1,
                deadlines: NROS_MODE_DEADLINES_1,
                switch_budget_ms: 50u32,
                reaction: "//enter_safe",
            },
        ];

        /// Install the mode variants AFTER `nros_install_monitors`; then
        /// `request_mode(id)` switches this executor at its next spin.
        pub fn nros_install_mode_tables(
            executor: &mut crate::executor::Executor<'_>,
        ) -> ::core::result::Result<(), crate::executor::monitor::ModeTablesMismatch> {
            executor.set_mode_tables(NROS_MODE_TABLES)
        }
    }

    /// The text `contract_monitors::render_monitor_rs_full` emits for the
    /// W3a/W3b fixture (one sensor, one handler, one hazard), verbatim but for
    /// the crate root. It is here so the GENERATED shape is compiled against
    /// the real types: a renderer change that stops compiling has to change
    /// this copy too, in plain sight.
    #[allow(dead_code, non_upper_case_globals)]
    mod rendered {
        use crate::executor::monitor::{
            AgeMonitorSpec, MonitorSpec, PubMonitorCell, SubMonitorCell,
        };

        static NROS_MONITOR_CELL_0: PubMonitorCell = PubMonitorCell::new();
        static NROS_MONITOR_CELL_1: PubMonitorCell = PubMonitorCell::new();
        static NROS_AGE_CELL_0: SubMonitorCell = SubMonitorCell::new();

        pub static NROS_MONITORS: &[MonitorSpec] = &[
            MonitorSpec {
                topic: "/stop",
                fqn: "/handler/stop",
                min_rate_hz_milli: 0u32,
                max_latency_ms: 20u32,
                route_trigger: "/scan",
                route_max_latency_ms: 20u32,
                route_deadline_ms: 0u32,
                reaction_id: 0u16,
                mechanism: 0u8,
                cell: &NROS_MONITOR_CELL_0,
            },
            MonitorSpec {
                topic: "/scan",
                fqn: "/sensor/scan",
                min_rate_hz_milli: 10000u32,
                max_latency_ms: 0u32,
                route_trigger: "",
                route_max_latency_ms: 0u32,
                route_deadline_ms: 0u32,
                reaction_id: 0u16,
                mechanism: 0u8,
                cell: &NROS_MONITOR_CELL_1,
            },
        ];

        pub static NROS_AGE_MONITORS: &[AgeMonitorSpec] = &[AgeMonitorSpec {
            topic: "/scan",
            fqn: "/handler/scan",
            max_age_ms: 100u32,
            deadline_ms: 0,
            cell: &NROS_AGE_CELL_0,
        }];

        use crate::executor::monitor::{GuardRef, GuardSpec, HazardCell, HazardSpec};

        static NROS_HAZARD_CELL_0: HazardCell = HazardCell::new();
        const NROS_HAZARD_GUARDS_0: &[GuardSpec] = &[GuardSpec {
            all_of: false,
            members: &[GuardRef::Age(0u16), GuardRef::Rate(1u16)],
        }];

        pub static NROS_HAZARDS: &[HazardSpec] = &[HazardSpec {
            key: "demo.launch.xml/scan_lost",
            ftti_ms: 0u32,
            settle_ms: 30u32,
            guards: NROS_HAZARD_GUARDS_0,
            sink: 0u16,
            reaction_id: 1u16,
            cell: &NROS_HAZARD_CELL_0,
        }];
    }

    /// The rendered tables drive the observer: the age row's silence is the
    /// hazard, and the sink row the renderer chose is the one it watches.
    #[test]
    fn the_rendered_include_drives_the_observer() {
        let spec = &rendered::NROS_HAZARDS[0];
        let mon = [MonitorState::default(); 2];
        let mut age = [AgeState::default(); 1];
        age[0].silence_reported = true;
        age[0].last_take_ms = 40;
        let mut out = std::vec::Vec::new();
        check_hazard(spec, rendered::NROS_MONITORS, &mon, &age, 100, |v| {
            out.push(v)
        });
        assert_eq!((out[0].rule, out[0].measured), ("hazard-detected", 60));
        assert_eq!(
            rendered::NROS_MONITORS[spec.sink as usize].fqn,
            "/handler/stop"
        );
    }

    /// With no sink the observer never claims a reaction it cannot see.
    #[test]
    fn no_sink_no_reaction() {
        static CELL: HazardCell = HazardCell::new();
        static GUARDS: [GuardSpec; 1] = [GuardSpec {
            all_of: false,
            members: &[GuardRef::Rate(0)],
        }];
        let spec = HazardSpec {
            key: "k",
            ftti_ms: 100,
            settle_ms: 0,
            guards: &GUARDS,
            sink: NO_SINK,
            reaction_id: 0,
            cell: &CELL,
        };
        let mut mon = [MonitorState::default(); 2];
        mon[0].violated_last_window = true;
        let v = run(&spec, &mon, &[], 50);
        assert_eq!((v[0].rule, v[0].measured), ("hazard-detected", 0));
        PUB_CELL.count.fetch_add(5, Ordering::Relaxed);
        assert!(run(&spec, &mon, &[], 60).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CELL: PubMonitorCell = PubMonitorCell::new();

    /// phase-496 W1 -- the Autoware Safety Island's shape: 14 rate rows and
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
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
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

    /// phase-474 I8 -- the island's first window after arming read 10 Hz
    /// publishers at 9984 mHz and never again. The monitor tick runs at the
    /// top of a spin, before that spin's dispatch publishes, so in steady
    /// state every window boundary sits one spin after a publish and the
    /// count matches the span. The window arming opens sits wherever the
    /// arming spin fell: off the publisher's phase, it spans 50 publishes
    /// over 5007 ms and reads short. A stream at exactly its declared rate,
    /// armed mid-period, must not be judged slow.
    #[test]
    fn a_stream_at_its_rate_armed_mid_period_is_not_judged_slow() {
        static C8: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/i8",
            fqn: "/n/i8",
            min_rate_hz_milli: 10_000, // 10 Hz declared
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &C8,
        };
        // Published at 0, 100 ms, ..., 50 s: 501 samples before arming.
        C8.count.store(501, Ordering::Relaxed);
        let mut st = MonitorState::default();
        // Arming spin, off the 10 ms grid (a wake for another entry).
        assert!(check_rate(&s, &mut st, 50_003_000).is_none());
        let mut t = 50_010_000u64;
        while t <= 80_000_000 {
            // The tick, then the spin's dispatch: the 10 Hz timer publishes.
            let v = check_rate(&s, &mut st, t);
            assert!(v.is_none(), "judged slow at {t} us: {v:?}");
            if t.is_multiple_of(100_000) {
                C8.count.fetch_add(1, Ordering::Relaxed);
            }
            t += 10_000;
        }
        // And a stream that IS slow is still judged in the first window:
        // half its rate from arming on.
        static C9: PubMonitorCell = PubMonitorCell::new();
        let slow = MonitorSpec { cell: &C9, ..s };
        let mut st = MonitorState::default();
        assert!(check_rate(&slow, &mut st, 50_003_000).is_none());
        let mut t = 50_010_000u64;
        let mut fired = None;
        while t <= 60_000_000 && fired.is_none() {
            fired = check_rate(&slow, &mut st, t).map(|v| (t, v.measured));
            if t.is_multiple_of(200_000) {
                C9.count.fetch_add(1, Ordering::Relaxed);
            }
            t += 10_000;
        }
        let (at, measured) = fired.expect("a 5 Hz stream against 10 Hz is judged");
        assert!(measured <= 5_100, "{measured}");
        // Judged one window after its first sample: at most one of its
        // periods (200 ms) plus a tick later than the window arming opened.
        assert!(at <= 50_003_000 + RATE_CHECK_INTERVAL_US + 210_000, "{at}");
        // A stream silent from arming on is judged at the first window's end,
        // not held open waiting for a sample that never comes.
        static C10: PubMonitorCell = PubMonitorCell::new();
        let dead = MonitorSpec { cell: &C10, ..s };
        let mut st = MonitorState::default();
        assert!(check_rate(&dead, &mut st, 50_003_000).is_none());
        let v = check_rate(&dead, &mut st, 50_003_000 + RATE_CHECK_INTERVAL_US)
            .expect("silent stream judged");
        assert_eq!(v.measured, 0);
    }

    /// phase-474 I9 -- the island's 10 Hz publishers read 9999 of 10000 mHz
    /// in every window once I8 aligned the first one: the spin that closes a
    /// window runs some hundreds of microseconds after the interval, so the
    /// quotient floors under the declared rate while the count is exact. The
    /// count judges; a stream at its declared rate checked late is not slow,
    /// a stream at half its rate still is.
    #[test]
    fn a_stream_at_its_rate_checked_late_is_not_judged_slow() {
        static C11: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/i9",
            fqn: "/n/i9",
            min_rate_hz_milli: 10_000,
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &C11,
        };
        let mut st = MonitorState::default();
        // Publishes at 0, 100 ms, ...; the monitor tick runs 700 us after
        // each publish, so every window spans the interval plus 700 us.
        let mut t = 0u64;
        while t <= 60_000_000 {
            if t.is_multiple_of(100_000) {
                C11.count.fetch_add(1, Ordering::Relaxed);
            }
            let v = check_rate(&s, &mut st, t + 700);
            assert!(v.is_none(), "judged slow at {t} us: {v:?}");
            t += 10_000;
        }
        // Half the rate, checked the same way: judged, measured about 5 Hz.
        static C12: PubMonitorCell = PubMonitorCell::new();
        let slow = MonitorSpec { cell: &C12, ..s };
        let mut st = MonitorState::default();
        let mut t = 0u64;
        let mut fired = None;
        while t <= 20_000_000 && fired.is_none() {
            if t.is_multiple_of(200_000) {
                C12.count.fetch_add(1, Ordering::Relaxed);
            }
            fired = check_rate(&slow, &mut st, t + 700).map(|v| v.measured);
            t += 10_000;
        }
        let measured = fired.expect("a 5 Hz stream against 10 Hz is judged");
        assert!((4_900..=5_100).contains(&measured), "{measured}");
    }

    #[test]
    fn compliant_and_uncontracted_stay_silent() {
        static C2: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            min_rate_hz_milli: 500, // 0.5 Hz
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
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
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
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
            deadline_ms: 0,
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
            deadline_ms: 0,
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
            deadline_ms: 0,
            cell: &SC3,
        };
        let mut st = AgeState::default();
        assert!(check_age(&uncontracted, &mut st, Some(0)).is_none());
        assert!(check_age(&uncontracted, &mut st, Some(10_000_000)).is_none());

        // Contracted and fed on every tick: silent rule, silent drain.
        let s = AgeMonitorSpec {
            max_age_ms: 100,
            deadline_ms: 0,
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
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
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
/// phase-474 I11 -- for a `spin_once(period)` loop with no declared cadence
/// the lateness is release minus scheduled release on the executor's release
/// grid (a dropped release counts its age when dropped), not entry-to-entry
/// minus the period; see `Executor::record_release_jitter`.
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

/// phase10-B3 -- the route rule's pure half: binding, the earliest unanswered
/// take, judging only on a publish of the output, report once until clean.
#[cfg(test)]
mod route_tests {
    use super::*;

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static OTHER: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 2] = [
        MonitorSpec {
            topic: "/out",
            fqn: "/n/out",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: "/in",
            route_max_latency_ms: 149,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &CELL,
        },
        MonitorSpec {
            topic: "/other",
            fqn: "/n/other",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &OTHER,
        },
    ];

    fn snapshot() -> [u32; MAX_MONITORS] {
        let mut c = [0u32; MAX_MONITORS];
        for (k, s) in TABLE.iter().enumerate() {
            c[k] = s.cell.count.load(Ordering::Relaxed);
        }
        c
    }

    fn judge(end_us: u64, before: &[u32; MAX_MONITORS]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        route_judge(&TABLE, before, end_us, |_, v, record, react| {
            assert!(record && !react, "an unsupervised route only records");
            assert_eq!(v.rule, "route-latency-runtime");
            assert_eq!(v.fqn, "/n/out");
            out.push((v.measured, v.declared));
        });
        out
    }

    extern crate alloc;
    use alloc::vec::Vec;

    #[test]
    fn route_rule() {
        assert!(route_active(&TABLE));
        route_bind(&TABLE, "/elsewhere", 1);
        assert_eq!(
            CELL.route.load(Ordering::Relaxed),
            0,
            "wrong topic binds nothing"
        );
        route_bind(&TABLE, "/in", 3);
        route_bind(&TABLE, "/in", 5);
        assert_eq!(
            CELL.route.load(Ordering::Relaxed),
            4,
            "first subscription keeps it"
        );
        assert_eq!(OTHER.route.load(Ordering::Relaxed), 0);

        // A take on another slot starts nothing; two takes keep the earliest.
        route_take(&TABLE, 5, 1_000);
        assert_eq!(CELL.route.load(Ordering::Relaxed) & ROUTE_PENDING, 0);
        route_take(&TABLE, 3, 10_000);
        route_take(&TABLE, 3, 90_000);
        // A dispatch that publishes nothing judges nothing.
        let b = snapshot();
        assert!(judge(400_000, &b).is_empty());
        assert_ne!(CELL.route.load(Ordering::Relaxed) & ROUTE_PENDING, 0);
        // The output published: 10 ms -> 160 ms is 150 ms, over 149.
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert_eq!(judge(160_000, &b), [(150, 149)]);
        assert_eq!(
            CELL.route.load(Ordering::Relaxed) & ROUTE_PENDING,
            0,
            "answered"
        );
        // Over again: reported once until a route is within the budget.
        route_take(&TABLE, 3, 200_000);
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert!(
            judge(400_000, &b).is_empty(),
            "still over: not reported twice"
        );
        route_take(&TABLE, 3, 500_000);
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert!(judge(600_000, &b).is_empty(), "100 ms: within");
        assert_eq!(CELL.route.load(Ordering::Relaxed) & ROUTE_VIOLATED, 0);
        route_take(&TABLE, 3, 700_000);
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert_eq!(
            judge(1_000_000, &b),
            [(300, 149)],
            "over after clean: reported"
        );
        // A publish with no waiting take is not a route.
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert!(judge(9_000_000, &b).is_empty());
        // Arming drops a waiting take and the report-once state, keeps the
        // binding; a release unbinds.
        route_take(&TABLE, 3, 9_100_000);
        route_reset(&TABLE);
        assert_eq!(CELL.route.load(Ordering::Relaxed), 4);
        route_unbind(&TABLE, 3);
        assert_eq!(CELL.route.load(Ordering::Relaxed), 0);
        // Wrapping: the stamp is the clock's low 32 bits.
        route_bind(&TABLE, "/in", 3);
        route_take(&TABLE, 3, u32::MAX as u64 - 9_999);
        let b = snapshot();
        CELL.count.fetch_add(1, Ordering::Relaxed);
        assert!(
            judge(u32::MAX as u64 + 90_001, &b).is_empty(),
            "100 ms across the wrap"
        );
    }

    #[test]
    fn route_rule_code_is_appended() {
        assert_eq!(rule_code("route-latency-runtime"), 10);
        assert_eq!(rule_code("release-jitter-runtime"), 9);
        assert_eq!(rule_code("route-deadline-runtime"), 11);
    }

    static DL_CELL: PubMonitorCell = PubMonitorCell::new();
    static DL_PLAIN: PubMonitorCell = PubMonitorCell::new();
    static DL_TABLE: [MonitorSpec; 2] = [
        // A route with the budget only: judged on completion, never overdue.
        MonitorSpec {
            topic: "/plain",
            fqn: "/n/plain",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: "/in",
            route_max_latency_ms: 149,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &DL_PLAIN,
        },
        // phase-503 -- the same route, supervised.
        MonitorSpec {
            topic: "/out",
            fqn: "/n/out",
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: "/in",
            route_max_latency_ms: 149,
            route_deadline_ms: 149,
            reaction_id: 3,
            mechanism: MECHANISM_TIMER,
            cell: &DL_CELL,
        },
    ];

    fn overdue(now_us: u64) -> Vec<(usize, u32, u32, bool)> {
        let mut out = Vec::new();
        route_overdue(&DL_TABLE, now_us, |row, v, first| {
            assert_eq!(v.rule, "route-deadline-runtime");
            assert_eq!(v.fqn, "/n/out");
            out.push((row, v.measured, v.declared, first));
        });
        out
    }

    /// phase-503 -- an output that never comes is judged at its deadline, not
    /// never: the wake source names the due time, the check at due reports it
    /// once with the row the hook is called for, and the next take starts a
    /// fresh route.
    #[test]
    fn route_deadline_rule() {
        assert!(route_deadline_active(&DL_TABLE));
        assert!(
            !route_deadline_active(&TABLE),
            "phase10-B3 rows stay unsupervised"
        );
        assert_eq!(route_next_due_us(&DL_TABLE, 0), None, "nothing pending");
        route_bind(&DL_TABLE, "/in", 2);
        route_take(&DL_TABLE, 2, 1_000_000);
        // The wake source: 149 ms after the take, then counting down.
        assert_eq!(route_next_due_us(&DL_TABLE, 1_000_000), Some(149_000));
        assert_eq!(route_next_due_us(&DL_TABLE, 1_100_000), Some(49_000));
        // One microsecond early: not overdue, still pending.
        assert!(overdue(1_148_999).is_empty());
        assert_ne!(DL_CELL.route.load(Ordering::Relaxed) & ROUTE_PENDING, 0);
        assert_eq!(route_next_due_us(&DL_TABLE, 1_148_999), Some(1));
        // At the deadline plus one 1 ms tick: reported for row 1, first.
        assert_eq!(overdue(1_150_000), [(1, 150, 149, true)]);
        let w = DL_CELL.route.load(Ordering::Relaxed);
        assert_eq!(w & ROUTE_PENDING, 0, "the take is dropped");
        assert_ne!(w & ROUTE_VIOLATED, 0);
        // The unsupervised row keeps its take for the completion verdict.
        assert_ne!(DL_PLAIN.route.load(Ordering::Relaxed) & ROUTE_PENDING, 0);
        assert_eq!(route_next_due_us(&DL_TABLE, 1_150_000), None);
        assert!(overdue(5_000_000).is_empty(), "judged once per take");
        // Still never answered: the next take misses too. The reaction is
        // requested again, the violation is not recorded twice.
        route_take(&DL_TABLE, 2, 6_000_000);
        assert_eq!(overdue(6_200_000), [(1, 200, 149, false)]);
        // A take answered in time clears the mark (route_judge's recovery).
        route_take(&DL_TABLE, 2, 7_000_000);
        let mut before = [0u32; MAX_MONITORS];
        for (k, s) in DL_TABLE.iter().enumerate() {
            before[k] = s.cell.count.load(Ordering::Relaxed);
        }
        DL_CELL.count.fetch_add(1, Ordering::Relaxed);
        let mut judged = Vec::new();
        route_judge(&DL_TABLE, &before, 7_010_000, |_, v, _, _| {
            judged.push(v.measured)
        });
        assert!(judged.is_empty(), "10 ms: within");
        assert_eq!(DL_CELL.route.load(Ordering::Relaxed) & ROUTE_VIOLATED, 0);
        // Over again after recovery: first again.
        route_take(&DL_TABLE, 2, 8_000_000);
        assert_eq!(overdue(8_149_000), [(1, 149, 149, true)]);
        // Across the clock's 32-bit wrap.
        route_take(&DL_TABLE, 2, u32::MAX as u64 - 9_999);
        assert_eq!(
            route_next_due_us(&DL_TABLE, u32::MAX as u64 + 1),
            Some(139_000)
        );
        assert!(overdue(u32::MAX as u64 + 139_000).is_empty());
        assert_eq!(overdue(u32::MAX as u64 + 139_001).len(), 1);
    }

    static LC_CELL: PubMonitorCell = PubMonitorCell::new();
    static LC_TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/out",
        fqn: "/n/late",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        route_trigger: "/in",
        route_max_latency_ms: 149,
        route_deadline_ms: 149,
        reaction_id: 5,
        mechanism: MECHANISM_EXECUTOR,
        cell: &LC_CELL,
    }];

    /// `(row, rule, measured, declared, record, react)` of one `route_judge`
    /// call after the output was published once.
    #[allow(clippy::type_complexity)]
    fn complete(end_us: u64) -> Vec<(usize, &'static str, u32, u32, bool, bool)> {
        let mut before = [0u32; MAX_MONITORS];
        before[0] = LC_CELL.count.load(Ordering::Relaxed);
        LC_CELL.count.fetch_add(1, Ordering::Relaxed);
        let mut out = Vec::new();
        route_judge(&LC_TABLE, &before, end_us, |row, v, record, react| {
            assert_eq!(v.fqn, "/n/late");
            out.push((row, v.rule, v.measured, v.declared, record, react));
        });
        out
    }

    fn lc_overdue(now_us: u64) -> usize {
        let mut n = 0;
        route_overdue(&LC_TABLE, now_us, |_, _, _| n += 1);
        n
    }

    /// phase-503 -- a supervised route whose one long callback publishes the
    /// output after the deadline, before the executor's overdue check could
    /// run: the completion reports the deadline miss and requests the
    /// reaction, once per take; a deadline already caught by the check is not
    /// reported again at completion; an on-time completion reports nothing.
    #[test]
    fn a_late_completion_is_a_deadline_miss_once() {
        route_bind(&LC_TABLE, "/in", 0);
        // Late completion, no overdue check in between (the callback held the
        // spin thread): the latency verdict, plus one deadline verdict that
        // asks for the reaction.
        route_take(&LC_TABLE, 0, 1_000_000);
        assert_eq!(
            complete(1_327_000),
            [
                (0, "route-latency-runtime", 327, 149, true, false),
                (0, "route-deadline-runtime", 327, 149, true, true),
            ]
        );
        assert_eq!(LC_CELL.route.load(Ordering::Relaxed) & ROUTE_PENDING, 0);
        assert_ne!(LC_CELL.route.load(Ordering::Relaxed) & ROUTE_VIOLATED, 0);
        // The take was answered: the overdue check has nothing left.
        assert_eq!(lc_overdue(1_400_000), 0, "no second verdict");
        // Late again while still marked: the reaction again, no record.
        route_take(&LC_TABLE, 0, 2_000_000);
        assert_eq!(
            complete(2_200_000),
            [(0, "route-deadline-runtime", 200, 149, false, true)]
        );
        // On time: nothing, and the mark clears.
        route_take(&LC_TABLE, 0, 3_000_000);
        assert!(complete(3_100_000).is_empty());
        assert_eq!(LC_CELL.route.load(Ordering::Relaxed) & ROUTE_VIOLATED, 0);
        // The deadline already fired (the check ran in time, or the timer
        // mechanism's): the late output that follows judges nothing.
        route_take(&LC_TABLE, 0, 4_000_000);
        assert_eq!(lc_overdue(4_150_000), 1);
        assert!(
            complete(4_327_000).is_empty(),
            "no second call at completion"
        );
        // Exactly at the deadline counts as overdue, as in route_overdue;
        // within the budget and before it does not.
        route_take(&LC_TABLE, 0, 5_000_000);
        let at = complete(5_149_000);
        assert_eq!(at.len(), 1);
        assert_eq!(at[0].1, "route-deadline-runtime");
        route_take(&LC_TABLE, 0, 6_000_000);
        assert!(complete(6_148_999).is_empty());
    }
}

/// phase-462 W5b -- the three admission outcomes, decided from the backend's
/// own mask, and the arrival evidence an armed row reads.
#[cfg(test)]
mod unserved_deadline_tests {
    use super::*;
    use nros_rmw::{QoSPolicyMask, QoSProfile};

    static CELL: SubMonitorCell = SubMonitorCell::new();
    static ROWS: [AgeMonitorSpec; 1] = [AgeMonitorSpec {
        topic: "/scan",
        fqn: "/h/scan",
        max_age_ms: 0,
        deadline_ms: 50,
        cell: &CELL,
    }];
    const ALL: QoSPolicyMask = QoSPolicyMask(u32::MAX);
    const NO_DEADLINE: QoSPolicyMask = QoSPolicyMask(!QoSPolicyMask::DEADLINE.0);

    fn with_deadline(ms: u32) -> QoSProfile {
        QoSProfile {
            deadline_ms: ms,
            ..QoSProfile::default()
        }
    }

    #[test]
    fn a_backend_that_honours_a_deadline_it_was_given_judges_it() {
        let (qos, row) = admit_contract_deadline(with_deadline(50), ALL, &ROWS, "/scan");
        assert_eq!((qos.deadline_ms, row.is_none()), (50, true));
    }

    #[test]
    fn an_unserved_contract_deadline_is_stripped_and_armed() {
        let (qos, row) = admit_contract_deadline(with_deadline(50), NO_DEADLINE, &ROWS, "/scan");
        assert_eq!(qos.deadline_ms, 0, "stripped, so create does not refuse it");
        assert_eq!(row.map(|r| r.fqn), Some("/h/scan"));
    }

    #[test]
    fn a_deadline_never_handed_to_the_backend_is_armed() {
        let (qos, row) = admit_contract_deadline(QoSProfile::default(), ALL, &ROWS, "/scan");
        assert_eq!((qos.deadline_ms, row.is_some()), (0, true));
    }

    /// The negative controls: the caller's OWN different deadline is left for
    /// the backend to refuse by name, and a topic with no row is untouched.
    #[test]
    fn a_callers_own_deadline_and_an_uncontracted_topic_are_left_alone() {
        let (qos, _) = admit_contract_deadline(with_deadline(20), NO_DEADLINE, &ROWS, "/scan");
        assert_eq!(qos.deadline_ms, 20);
        let (qos, row) = admit_contract_deadline(with_deadline(50), NO_DEADLINE, &ROWS, "/other");
        assert_eq!((qos.deadline_ms, row.is_none()), (50, true));
    }

    #[test]
    fn readiness_credits_only_the_armed_slot_and_unbind_disarms() {
        static C: SubMonitorCell = SubMonitorCell::new();
        static T: [AgeMonitorSpec; 1] = [AgeMonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            max_age_ms: 0,
            deadline_ms: 10,
            cell: &C,
        }];
        credit_arrivals(&T, 1 << 3);
        assert_eq!(C.arrivals.load(Ordering::Relaxed), 0, "not armed");
        C.deadline_slot.store(4, Ordering::Relaxed);
        credit_arrivals(&T, 1 << 2);
        credit_arrivals(&T, 1 << 3);
        assert_eq!(C.arrivals.load(Ordering::Relaxed), 1);
        deadline_unbind(&T, 2);
        assert_eq!(C.deadline_slot.load(Ordering::Relaxed), 4, "another slot");
        deadline_unbind(&T, 3);
        assert_eq!(C.deadline_slot.load(Ordering::Relaxed), 0);
    }

    /// An armed deadline-only row judges arrivals over the deadline; fed, it
    /// stays quiet. Unarmed, the same row judges nothing at all.
    #[test]
    fn an_armed_deadline_row_judges_arrivals_over_its_window() {
        static C: SubMonitorCell = SubMonitorCell::new();
        let spec = AgeMonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            max_age_ms: 0,
            deadline_ms: 10,
            cell: &C,
        };
        let mut st = AgeState::default();
        assert!(check_age(&spec, &mut st, Some(1_000_000)).is_none());
        assert!(
            check_age(&spec, &mut st, Some(1_050_000)).is_none(),
            "unarmed"
        );
        C.deadline_slot.store(1, Ordering::Relaxed);
        let mut st = AgeState::default();
        assert!(
            check_age(&spec, &mut st, Some(2_000_000)).is_none(),
            "opens"
        );
        C.arrivals.store(1, Ordering::Relaxed);
        assert!(check_age(&spec, &mut st, Some(2_020_000)).is_none(), "fed");
        let v = check_age(&spec, &mut st, Some(2_040_000)).expect("silent past 10 ms");
        assert_eq!((v.rule, v.declared), ("silence-runtime", 10));
    }
}

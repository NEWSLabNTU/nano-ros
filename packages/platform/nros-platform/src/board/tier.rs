//! Per-tier scheduling descriptors — Phase 228.E (RFC-0015 execution
//! model, RFC-0016 priority mapping).
//!
//! A [`TierSpec`] names one RTOS task that an `Executor` will run on a
//! shared RMW session. The orchestration `main()` (codegen-emitted)
//! passes a `&[TierSpec]` to the board's `run_tiers(...)`; the board
//! opens the session once, then spawns one task per spec — each task
//! opens an `Executor` over the *same* session (the `Borrowed` store),
//! sets its `active_groups` filter, registers nodes (only its tier's
//! callbacks take), and spins. The LEAST urgent tier runs on the boot
//! task itself ([`boot_tier_index`]); the rest are spawned.
//!
//! Issue 0636 — that last sentence used to read "the highest-priority tier
//! runs on the boot task", and it was both wrong and harmful. Boards took
//! `tiers[0]`, and `resolve_tiers` orders by RAW number descending WITHOUT
//! inverting per kernel, so `tiers[0]` is the most urgent tier on
//! bigger-number-wins kernels (NuttX, FreeRTOS, POSIX) and the least urgent on
//! smaller-number-wins ones (Zephyr, ThreadX). Which tier owned the session
//! therefore depended on the kernel's number direction, which nobody chose.
//!
//! On a uniprocessor with FIFO scheduling, an owner that outranks its peers and
//! then spins starves them: a lower-priority tier runs only in whatever gap the
//! owner's `spin_once` happens to leave. Measured on NuttX at 1 of 5 runs
//! reaching a spawned tier's first statement at all. `sched_yield` cannot fix
//! it — under SCHED_FIFO a yield rotates the caller within its OWN priority
//! queue and never lets a lower-priority thread run, which is why two partial
//! fixes moved the rate to 4 of 6 and could not converge.
//!
//! So the owner is chosen, not inherited from an ordering: it is the tier that
//! outranks nothing.
//!
//! Priorities are declared on a normalized **0–31** scale (RFC-0016):
//! 0 = idle, 12 = normal (default app), 31 = critical. The per-RTOS
//! mappers below lower that to each kernel's native range. Keeping the
//! scale RTOS-agnostic lets the same `system.toml [tiers.*]` deploy
//! across families without rewriting priorities.

/// One scheduling tier: an RTOS task running an `Executor` over the
/// shared session, admitting only the listed callback groups.
///
/// All fields are literal-constructible so the codegen emitter can bake
/// a `const`/`static` array of these straight from the resolved tier
/// table in `nros-plan.json`.
#[derive(Clone, Copy, Debug)]
pub struct TierSpec<'a> {
    /// Tier name (matches the `system.toml [tiers.<name>]` key); used
    /// for the spawned task's debug name.
    pub name: &'a str,
    /// Callback groups admitted on this tier. Passed verbatim to
    /// `Executor::set_active_groups`; an empty slice = wildcard
    /// (admit every group — the single-tier degenerate case).
    pub groups: &'a [&'a str],
    /// **Raw per-RTOS** task priority — the value passed straight to the
    /// native spawn call. The system author writes it in
    /// `[tiers.<name>.<rtos>].priority`, so it is already in the target
    /// kernel's scale (FreeRTOS 0–7, ThreadX 0–31 lower=higher, …);
    /// `i64` admits Zephyr's negative coop priorities. (The
    /// `*_priority_for` mappers in this module are a separate utility for
    /// authors who prefer a normalized 0–31 scale; the codegen path uses
    /// the raw value verbatim.)
    pub priority: i64,
    /// Task stack size in bytes. `0` = let the board pick its default.
    pub stack_bytes: usize,
    /// Spin period for this tier's `spin_once` loop, in microseconds.
    pub spin_period_us: u64,
    // -- RFC-0052 / phase-296 W2 — the previously-dropped tier fields ride
    // -- the spec end-to-end. Boards consume what their kernel offers; the
    // -- bake already rejected platform-inapplicable knobs (fail-loud), so
    // -- an unconsumed Some(..) here is a board TODO, not a silent config
    // -- loss.
    /// CPU core to pin the tier task to (SMP boards); `None` = unpinned.
    pub core: Option<u32>,
    /// ThreadX preemption threshold (ThreadX targets only; bake-validated).
    pub preempt_threshold: Option<i64>,
    /// Round-robin time slice in µs (#0266): `Some` requests time-slicing among
    /// same-priority tiers. ThreadX-only today (bake-validated); `None` = FIFO.
    pub time_slice_us: Option<u64>,
    /// Scheduling class: `"best_effort"` | `"real_time"` |
    /// `"time_triggered"` (bake rejects `"interrupt"`); `None` = plain
    /// priority tier.
    pub class: Option<&'a str>,
    /// Callback period (µs) — `time_triggered` window period / sporadic
    /// replenishment period.
    pub period_us: Option<u64>,
    /// Execution-time budget (µs) — sporadic-server budget (W3 wires it
    /// into the executor's `SchedContext`).
    pub budget_us: Option<u64>,
    /// Relative deadline (µs) for the deadline monitor (W3).
    pub deadline_us: Option<u64>,
    /// On deadline miss: `"ignore"` | `"warn"` | `"skip"` | `"fault"`.
    pub deadline_policy: Option<&'a str>,
}

/// Which way a kernel's raw priority numbers run.
///
/// `TierSpec::priority` is RAW — the number the system author wrote in
/// `[tiers.<name>.<rtos>].priority`, already in the target kernel's scale — so
/// only the board knows which end is urgent. Issue 0636.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorityDirection {
    /// NuttX (1..=255), FreeRTOS (0..=7), POSIX SCHED_FIFO: bigger wins.
    BiggerIsMoreUrgent,
    /// Zephyr (negative = cooperative), ThreadX (0..=31): smaller wins.
    SmallerIsMoreUrgent,
}

/// Index of the tier the BOOT task should run: the least urgent one.
///
/// Issue 0636 — the boot task owns the session and spins forever, so it must
/// not outrank the tiers it spawned. See the module docs for what happened when
/// it did.
///
/// * **Ties keep the earliest index**, so a table whose tiers all declare the
///   same priority (or none) behaves exactly as `tiers[0]` did.
/// * **An undeclared priority is least urgent, but only where 0 is out of
///   range.** On bigger-wins kernels the valid range starts at 1 and `0` is the
///   "inherit" sentinel every board already tests for, so a tier that declared
///   nothing makes no claim and is the safest owner. On smaller-wins kernels 0
///   is a REAL priority — a very urgent one on ThreadX — and treating it as a
///   sentinel would hand the session to the most urgent tier, which is the bug
///   this function exists to prevent.
#[must_use]
pub fn boot_tier_index(tiers: &[TierSpec<'_>], direction: PriorityDirection) -> usize {
    /// Bigger = more urgent, in one comparable scale.
    fn urgency(priority: i64, direction: PriorityDirection) -> i64 {
        match direction {
            PriorityDirection::BiggerIsMoreUrgent if priority <= 0 => i64::MIN,
            PriorityDirection::BiggerIsMoreUrgent => priority,
            // Negate rather than subtract: the range is the kernel's, and any
            // fixed origin would be one more number to keep in step with it.
            PriorityDirection::SmallerIsMoreUrgent => priority.saturating_neg(),
        }
    }
    let mut best = 0;
    for (i, tier) in tiers.iter().enumerate().skip(1) {
        if urgency(tier.priority, direction) < urgency(tiers[best].priority, direction) {
            best = i;
        }
    }
    best
}

impl<'a> TierSpec<'a> {
    /// A degenerate single tier: wildcard groups, normal priority, the
    /// board's default stack. Equivalent to today's single-task entry.
    pub const fn single() -> TierSpec<'static> {
        TierSpec {
            name: "default",
            groups: &[],
            priority: 0,
            stack_bytes: 0,
            spin_period_us: 1_000,
            core: None,
            preempt_threshold: None,
            time_slice_us: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
        }
    }
}

/// FreeRTOS native priority (0..=`configMAX_PRIORITIES-1`, here 0–7)
/// for a normalized 0–31 priority. RFC-0016 §Design: linear
/// interpolation `(n*7 + 15) / 31` (round-to-nearest), so 0→0 (idle)
/// and 31→7 (highest). Higher number = higher priority on FreeRTOS.
pub const fn freertos_priority_for(normalized: u8) -> u8 {
    let n = clamp31(normalized) as u32;
    ((n * 7 + 15) / 31) as u8
}

/// ThreadX native priority (0..=31, **lower = higher priority**) for a
/// normalized 0–31 priority. RFC-0016: inverted scale `31 - n`, so the
/// normalized idle (0) maps to ThreadX 31 (lowest) and normalized
/// critical (31) maps to ThreadX 0 (highest).
pub const fn threadx_priority_for(normalized: u8) -> u8 {
    31 - clamp31(normalized)
}

/// POSIX `nice` value (`-20`..=`19`, **lower = more CPU**) for a
/// normalized 0–31 priority. Best-effort: native preemption normally
/// uses the default scheduler (strict ordering needs `SCHED_FIFO` +
/// privileges), so this is an advisory niceness, linear over the scale
/// and clamped, with idle (0) pinned to the maximum `19`. Anchors track
/// the RFC-0016 table (12→0 normal, 31→-20 critical).
pub const fn posix_nice_for(normalized: u8) -> i32 {
    let n = clamp31(normalized) as i32;
    if n == 0 {
        return 19;
    }
    // Slope ≈ -1.25 nice/step around the normal anchor (n=12 → 0).
    let nice = (-5 * (n - 12)) / 4;
    if nice > 19 {
        19
    } else if nice < -20 {
        -20
    } else {
        nice
    }
}

// ============================================================================
// Issue 0636 option 3 — the tier spin loop's scheduled gap
// ============================================================================
//
// [`boot_tier_index`] stopped the starvation by making the session owner the
// tier that outranks nothing. That is correct and it is what fixed the issue,
// but the guarantee it gives rests entirely on the priority ORDER being right:
// any tier that outranks another and then spins without blocking can still hold
// a uniprocessor forever, because under SCHED_FIFO a thread yields the CPU only
// by blocking (`sched_yield` rotates within the caller's own priority queue and
// never reaches a lower one).
//
// Whether a spin blocks is, today, a property of the TRANSPORT rather than of
// the tier. `Executor::spin_once` takes the blocking arm only when nothing has
// already woken it:
//
//     if !was_woken && has_async_wake && node_wake => wake.wait_ms(timeout)
//     else                                        => drive_io(timeout_ms)
//
// The `was_woken` arm drives I/O with a ZERO timeout — deliberately, because a
// wake means there is data to drain. So under sustained arrival every iteration
// takes the non-blocking arm, and the loop has no blocking point exactly when
// the system is busiest. That is the "transport luck" this type removes.
//
// # The rule
//
// A tier loop may not run for longer than one INTERVAL without either blocking
// in its own spin or taking a bounded gap. Both halves are derived from what
// the author already declared, so there is no new knob:
//
// * **interval** = `max(spin_period_us, 10 ms)`. The gap costs 1 ms, so the
//   floor is what caps the worst-case overhead at 10 %; a tier that declares a
//   longer period pays proportionally less.
// * **"it blocked"** = an iteration took at least half the declared spin
//   period. A spin that waited its timeout is near the period; a free-running
//   one is orders of magnitude shorter. Half is the midpoint between them, not
//   a tuned constant.
// * **the gap** = `nros_platform_sleep_ms(1)`. Deliberately NOT `sleep_us`:
//   that one's ABI contract says it may SPIN when the platform has no
//   sub-millisecond timer, and a spin is not a scheduling point — it would
//   satisfy the code and not the requirement. 1 ms is the smallest sleep the
//   ABI guarantees actually blocks.
//
// Cost is zero on the blocking path: a loop whose spins wait never opens a
// window without a block in it, so it never sleeps here.
//
// # Why the state is one `u64`
//
// The C tier runners (`*_run_tiers.c`) and the Rust ones must share ONE
// implementation — a second spelling per language is how the tier-priority
// marker drifted in the first place (see the module docs above). A shared
// STRUCT would be a hand-mirrored FFI layout, which this repo has been bitten
// by three times, so the state is a single opaque `u64` that C keeps and passes
// back: the window's start timestamp with bit 0 carrying "something blocked in
// this window". One nanosecond of timestamp resolution is the whole cost.

/// Overhead ceiling for the gap: 1 ms of sleep per 10 ms of free-running spin.
const GAP_INTERVAL_FLOOR_US: u64 = 10_000;

/// The gap itself. The smallest sleep the platform ABI guarantees will BLOCK
/// rather than spin — see the module note on `sleep_us`.
const GAP_MS: usize = 1;

/// Gap window length for a tier that declared `spin_period_us`.
#[must_use]
pub const fn gap_interval_us(spin_period_us: u64) -> u64 {
    let p = spin_period_us;
    if p > GAP_INTERVAL_FLOOR_US {
        p
    } else {
        GAP_INTERVAL_FLOOR_US
    }
}

/// Did this iteration BLOCK? True when it lasted at least half the declared
/// spin period — see the module note for why half.
#[must_use]
pub const fn iteration_blocked(iter_ns: u64, spin_period_us: u64) -> bool {
    let half_us = spin_period_us / 2;
    // A tier declaring a period under 2 us has no meaningful "half"; treat any
    // measurable time as a block rather than gapping such a loop every window.
    if half_us == 0 {
        return iter_ns > 0;
    }
    iter_ns >= half_us.saturating_mul(1_000)
}

/// The whole decision, as a pure function of the clock — so it is testable on a
/// host with no platform linked, which is the only way the arithmetic below
/// gets exercised at all (every caller is an RTOS image).
///
/// Returns the next state and whether the caller must sleep. `state` is the
/// value returned by the previous call, or `0` to start a window.
#[must_use]
pub const fn gap_step(state: u64, iter_ns: u64, now_ns: u64, spin_period_us: u64) -> (u64, bool) {
    let window_start_ns = state & !STATE_FLAGS;
    let blocked_in_window =
        (state & STATE_BLOCKED) != 0 || iteration_blocked(iter_ns, spin_period_us);

    // First ever call: open a window at `now` and decide nothing yet.
    //
    // "Open" is its own BIT, not `state != 0`. The clock's epoch is
    // platform-defined and a port may legitimately hand out 0 (or 1) on the
    // first read — and with the timestamp alone as the state, such a port
    // re-entered this branch on every iteration and the gap never fired at all.
    // Found by the unit test below, not on a target.
    if state & STATE_OPEN == 0 {
        return (open_state(now_ns, blocked_in_window), false);
    }

    if now_ns.saturating_sub(window_start_ns) < gap_interval_us(spin_period_us) * 1_000 {
        return (open_state(window_start_ns, blocked_in_window), false);
    }

    // Window closed. Gap only if nothing in it blocked. Either way the next
    // window starts here; the caller re-stamps after sleeping (see `TierSpinGap`)
    // so the sleep is not charged to the window it opens.
    (open_state(now_ns, false), !blocked_in_window)
}

/// Window-open marker — see [`gap_step`] for why the timestamp alone will not do.
const STATE_OPEN: u64 = 1;
/// "Something blocked in this window."
const STATE_BLOCKED: u64 = 2;
const STATE_FLAGS: u64 = STATE_OPEN | STATE_BLOCKED;

/// Pack a window start (2 ns of resolution given up to the two flag bits).
#[must_use]
const fn open_state(window_start_ns: u64, blocked: bool) -> u64 {
    (window_start_ns & !STATE_FLAGS) | STATE_OPEN | if blocked { STATE_BLOCKED } else { 0 }
}

/// One tier spin loop's scheduled gap. Construct beside the loop, call
/// [`TierSpinGap::after_spin`] at the bottom of every iteration.
///
/// ```ignore
/// let mut gap = TierSpinGap::new(tier.spin_period_us);
/// loop {
///     let t0 = gap.mark();
///     crt.spin_once(period);
///     gap.after_spin(t0);
/// }
/// ```
#[derive(Debug)]
pub struct TierSpinGap {
    state: u64,
    spin_period_us: u64,
    gaps: u64,
}

unsafe extern "C" {
    fn nros_platform_clock_ns() -> u64;
    fn nros_platform_sleep_ms(ms: usize);
}

impl TierSpinGap {
    #[must_use]
    pub const fn new(spin_period_us: u64) -> Self {
        Self {
            state: 0,
            spin_period_us,
            gaps: 0,
        }
    }

    /// Timestamp for the start of an iteration.
    #[must_use]
    pub fn mark(&self) -> u64 {
        // SAFETY: a bare monotonic read with no preconditions, defined by
        // whichever platform port linked this image.
        unsafe { nros_platform_clock_ns() }
    }

    /// Close out one iteration, sleeping if this one closed a window in which
    /// nothing blocked.
    pub fn after_spin(&mut self, iter_start_ns: u64) {
        let now = self.mark();
        let (state, sleep) = gap_step(
            self.state,
            now.saturating_sub(iter_start_ns),
            now,
            self.spin_period_us,
        );
        self.state = state;
        if sleep {
            // SAFETY: no preconditions; blocks the calling task for >= 1 ms.
            unsafe { nros_platform_sleep_ms(GAP_MS) };
            self.gaps = self.gaps.saturating_add(1);
            // Re-stamp so the sleep itself is not charged to the new window.
            self.state = open_state(self.mark(), false);
        }
    }

    /// How many gaps this loop has taken — for the tier heartbeat, so a busy
    /// image can say whether the guarantee is being exercised or is dead code.
    #[must_use]
    pub const fn gaps(&self) -> u64 {
        self.gaps
    }
}

/// The same decision for the C tier runners, which keep the `u64` themselves.
///
/// Pass `0` on the first call. The sleep happens HERE, so the two languages
/// share one implementation of both halves of the rule.
///
/// # Safety
/// None beyond the platform ABI being linked, which is true in any image that
/// has tiers to run.
#[unsafe(no_mangle)]
pub extern "C" fn nros_tier_spin_gap_step(
    state: u64,
    iter_start_ns: u64,
    now_ns: u64,
    spin_period_us: u64,
) -> u64 {
    let (next, sleep) = gap_step(
        state,
        now_ns.saturating_sub(iter_start_ns),
        now_ns,
        spin_period_us,
    );
    if sleep {
        // SAFETY: no preconditions; blocks the calling task for >= 1 ms.
        unsafe { nros_platform_sleep_ms(GAP_MS) };
        // SAFETY: bare monotonic read.
        return open_state(unsafe { nros_platform_clock_ns() }, false);
    }
    next
}

const fn clamp31(n: u8) -> u8 {
    if n > 31 { 31 } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freertos_anchors_match_rfc0016() {
        // RFC-0016 table column FreeRTOS(0–7).
        assert_eq!(freertos_priority_for(0), 0); // idle
        assert_eq!(freertos_priority_for(12), 3); // normal
        assert_eq!(freertos_priority_for(20), 5); // high
        assert_eq!(freertos_priority_for(31), 7); // critical
        // Saturates above the scale.
        assert_eq!(freertos_priority_for(200), 7);
    }

    #[test]
    fn threadx_inverts_scale() {
        assert_eq!(threadx_priority_for(0), 31); // idle → lowest
        assert_eq!(threadx_priority_for(31), 0); // critical → highest
        assert_eq!(threadx_priority_for(12), 19);
    }

    #[test]
    fn posix_nice_anchors() {
        assert_eq!(posix_nice_for(0), 19); // idle pinned to max nice
        assert_eq!(posix_nice_for(12), 0); // normal
        assert_eq!(posix_nice_for(31), -20); // critical (clamped)
        assert!(posix_nice_for(20) < 0); // high → negative nice
    }

    #[test]
    fn single_tier_is_wildcard() {
        let t = TierSpec::single();
        assert!(t.groups.is_empty());
        assert_eq!(t.priority, 0);
    }

    fn spec(name: &'static str, priority: i64) -> TierSpec<'static> {
        TierSpec {
            name,
            priority,
            ..TierSpec::single()
        }
    }

    /// issue 0636 — the owner must be the tier that outranks nothing, and
    /// "nothing" depends on which end of the kernel's scale is urgent.
    #[test]
    fn boot_tier_is_the_least_urgent_on_either_direction() {
        // As `resolve_tiers` hands them over: RAW number, descending.
        let tiers = [spec("high", 110), spec("mid", 105), spec("low", 100)];

        // Bigger wins (NuttX, FreeRTOS, POSIX): 100 is least urgent. This is
        // the case that starved — the board used to take index 0 (110) and
        // spin there.
        assert_eq!(
            boot_tier_index(&tiers, PriorityDirection::BiggerIsMoreUrgent),
            2
        );
        // Smaller wins (Zephyr, ThreadX): 110 is least urgent, which is index
        // 0 — the arrangement those boards already had, now stated rather than
        // inherited from the sort direction.
        assert_eq!(
            boot_tier_index(&tiers, PriorityDirection::SmallerIsMoreUrgent),
            0
        );
    }

    /// A tier that declared nothing makes no claim, so it is the safest owner —
    /// but only where 0 is out of the kernel's range.
    #[test]
    fn undeclared_is_least_urgent_only_where_zero_is_a_sentinel() {
        let tiers = [spec("declared", 12), spec("undeclared", 0)];
        assert_eq!(
            boot_tier_index(&tiers, PriorityDirection::BiggerIsMoreUrgent),
            1
        );
        // On ThreadX 0 is a REAL priority, and the most urgent one. Treating it
        // as a sentinel would hand the session to the tier that must never own
        // it — the exact inversion this function exists to prevent.
        assert_eq!(
            boot_tier_index(&tiers, PriorityDirection::SmallerIsMoreUrgent),
            0
        );
    }

    // ---- issue 0636 option 3 — the spin gap ----

    /// What `TierSpinGap` does after sleeping: open a fresh window at `now`.
    fn open_state_for_test(now_ns: u64) -> u64 {
        super::open_state(now_ns, false)
    }

    /// The interval is the declared period, floored so the 1 ms gap can never
    /// cost more than 10 %.
    #[test]
    fn gap_interval_floors_at_ten_ms() {
        assert_eq!(gap_interval_us(200), 10_000); // 200 us tier: floored
        assert_eq!(gap_interval_us(10_000), 10_000); // exactly the floor
        assert_eq!(gap_interval_us(100_000), 100_000); // 100 ms tier: its own
    }

    /// "It blocked" is half the declared period — a waited spin lands near the
    /// period, a free-running one is orders of magnitude below it.
    #[test]
    fn blocked_is_half_the_declared_period() {
        // 10 ms tier: 5 ms counts, 4.9 ms does not.
        assert!(iteration_blocked(5_000_000, 10_000));
        assert!(!iteration_blocked(4_900_000, 10_000));
        // A free-running iteration (~2 us) never counts.
        assert!(!iteration_blocked(2_000, 10_000));
        // Degenerate sub-2 us period: any measurable time counts, so such a
        // loop is not gapped every window.
        assert!(iteration_blocked(1, 1));
        assert!(!iteration_blocked(0, 1));
    }

    /// A loop whose spins BLOCK never sleeps here — the whole point of keeping
    /// the cost off the healthy path.
    #[test]
    fn a_blocking_loop_never_gaps() {
        let period_us = 10_000;
        let mut state = 0u64;
        let mut now = 1_000_000_000u64;
        for _ in 0..500 {
            // Each iteration takes its full declared period.
            let iter_ns = period_us * 1_000;
            now += iter_ns;
            let (next, sleep) = gap_step(state, iter_ns, now, period_us);
            assert!(!sleep, "a spin that waited its period must not be gapped");
            state = next;
        }
    }

    /// A free-running loop gaps once per interval, and not more.
    #[test]
    fn a_free_running_loop_gaps_once_per_interval() {
        let period_us = 10_000; // interval = 10 ms
        let mut state = 0u64;
        let mut now = 42u64;
        let mut sleeps = 0;
        // 100 ms of 5 us iterations.
        for _ in 0..20_000 {
            let iter_ns = 5_000;
            now += iter_ns;
            let (next, sleep) = gap_step(state, iter_ns, now, period_us);
            if sleep {
                sleeps += 1;
                // The caller re-stamps after sleeping; model that.
                now += 1_000_000;
                state = open_state_for_test(now);
            } else {
                state = next;
            }
        }
        // 100 ms of spinning, 10 ms windows, 1 ms of sleep charged to none of
        // them: 9 or 10 depending on where the first window opens.
        assert!(
            (9..=10).contains(&sleeps),
            "expected ~one gap per 10 ms window, got {sleeps}"
        );
    }

    /// ONE blocking iteration is enough to spare the whole window — the rule is
    /// "the loop reached a scheduling point", not "every spin did".
    #[test]
    fn one_block_in_a_window_suppresses_its_gap() {
        let period_us = 10_000;
        let mut state = 0u64;
        let mut now = 1_000u64;
        let mut sleeps = 0;
        for i in 0..4_000 {
            // One blocking iteration every 1000 free-running ones, which is
            // more than one per 10 ms window at 5 us per iteration.
            let iter_ns = if i % 1_000 == 999 { 6_000_000 } else { 5_000 };
            now += iter_ns;
            let (next, sleep) = gap_step(state, iter_ns, now, period_us);
            if sleep {
                sleeps += 1;
            }
            state = next;
        }
        assert_eq!(sleeps, 0, "a window containing a real block must not gap");
    }

    /// The first call opens a window instead of deciding: a zero state cannot
    /// be read as "window opened at time 0", because the clock epoch is the
    /// platform's and a fresh image legitimately reads small values.
    #[test]
    fn first_call_opens_a_window_and_never_sleeps() {
        for now in [0u64, 1, 5_000_000, u64::MAX / 2] {
            let (state, sleep) = gap_step(0, 5_000, now, 10_000);
            assert!(!sleep, "the first iteration must not be gapped");
            assert_ne!(state, 0, "the window must be open after the first call");
        }
    }

    /// A port whose clock epoch IS zero must still gap. With the timestamp
    /// alone as the state, `now = 0` re-read as "not started yet" on every
    /// iteration and the guarantee silently did not exist on that port.
    #[test]
    fn a_clock_that_starts_at_zero_still_gaps() {
        let period_us = 10_000;
        let mut state = 0u64;
        let mut now = 0u64; // epoch, exactly
        let mut sleeps = 0;
        for _ in 0..10_000 {
            now += 5_000; // 5 us of free-running spin
            let (next, sleep) = gap_step(state, 5_000, now, period_us);
            if sleep {
                sleeps += 1;
                now += 1_000_000;
                state = open_state_for_test(now);
            } else {
                state = next;
            }
        }
        assert!(sleeps > 0, "a zero-epoch clock must not disable the gap");
    }

    /// Ties keep the earliest index, so a table with one tier — or with every
    /// tier equal — behaves exactly as `tiers[0]` did before issue 0636.
    #[test]
    fn ties_and_single_tier_keep_index_zero() {
        let same = [spec("a", 7), spec("b", 7), spec("c", 7)];
        for dir in [
            PriorityDirection::BiggerIsMoreUrgent,
            PriorityDirection::SmallerIsMoreUrgent,
        ] {
            assert_eq!(boot_tier_index(&same, dir), 0);
            assert_eq!(boot_tier_index(&same[..1], dir), 0);
        }
    }
}

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

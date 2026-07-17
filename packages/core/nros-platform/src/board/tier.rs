//! Per-tier scheduling descriptors — Phase 228.E (RFC-0015 execution
//! model, RFC-0016 priority mapping).
//!
//! A [`TierSpec`] names one RTOS task that an `Executor` will run on a
//! shared RMW session. The orchestration `main()` (codegen-emitted)
//! passes a `&[TierSpec]` to the board's `run_tiers(...)`; the board
//! opens the session once, then spawns one task per spec — each task
//! opens an `Executor` over the *same* session (the `Borrowed` store),
//! sets its `active_groups` filter, registers nodes (only its tier's
//! callbacks take), and spins. The highest-priority tier runs on the
//! boot task itself; the rest are spawned.
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
}

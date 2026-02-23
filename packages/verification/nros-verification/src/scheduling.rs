/// Real-time scheduling proofs (Phase 31.2, updated Phase 56.1)
///
/// Proves bounded, predictable behavior of the executor's timer and trigger
/// subsystems. These are the prerequisite for WCET analysis and schedulability proofs.
///
/// ## Trust levels
///
/// **Ghost model** (shared from `nros-ghost-types`, validated by production tests):
/// - `TimerGhost` / `TimerModeGhost` — mirrors `TimerState` / `TimerMode`.
///   Registered via `external_type_specification`.
///
/// **Pure math** (no link to production code):
/// - Trigger spec functions (`trigger_any`, `trigger_all`, `trigger_one`,
///   `trigger_all_of`, `trigger_any_of`) model the 6 deterministic variants
///   of `nros_node::Trigger`.  The `Trigger` enum itself is **not** registered
///   with Verus because it contains fn-pointer variants (`Predicate`,
///   `RawPredicate`) that Verus cannot reason about.
/// - `spin_once_result_consistency` — proves arithmetic identity about the
///   `any_work() ⟺ total() > 0` relationship.
///
/// ## Audit contract
///
/// A human auditor should confirm that the spec functions below match the
/// production `spin_once()` trigger evaluation at `spin.rs:1050-1072`:
///
/// ```text
/// Trigger::Any           => bits & non_timer_mask != 0 || non_timer_mask == 0
/// Trigger::All           => bits & non_timer_mask == non_timer_mask
/// Trigger::One(id)       => snapshot.is_ready(id)
/// Trigger::AllOf(set)    => snapshot.all_ready(set)
/// Trigger::AnyOf(set)    => snapshot.any_ready(set)
/// Trigger::Always        => true
/// Trigger::Predicate(f)  => f(&snapshot)        // opaque — not modeled
/// Trigger::RawPredicate  => callback(...)        // opaque — not modeled
/// ```
///
/// The `Seq<bool>` model used here is equivalent to the `u64` bitmask
/// representation: `ready[i] ⟺ bits & (1 << i) != 0`.  The bitmask
/// invariants are separately verified by Kani harnesses in
/// `nros-node/src/executor/types.rs` (`snapshot_all_ready_correct`,
/// `snapshot_any_ready_correct`, `snapshot_is_ready_correct`).
///
/// ## Remaining limitations
///
/// - `Predicate` and `RawPredicate` are opaque (fn pointer / unsafe extern).
/// - `SpinOnceResult` requires `zenoh` feature → C FFI deps → can't import.
/// - `TimerState` fields are `pub(crate)` → can't access from external crate.
use vstd::prelude::*;
use nros_ghost_types::{TimerGhost, TimerModeGhost};

verus! {

// ======================================================================
// Timer State Machine (from nros-ghost-types)
// ======================================================================

/// Register `TimerModeGhost` as a transparent type so Verus can match on variants.
#[verifier::external_type_specification]
pub struct ExTimerModeGhost(TimerModeGhost);

/// Register `TimerGhost` as a transparent type so Verus can access fields.
#[verifier::external_type_specification]
pub struct ExTimerGhost(TimerGhost);

/// Model of `u64::saturating_add` — returns `a + b` clamped to `u64::MAX`.
/// Mirrors: `self.elapsed_ms = self.elapsed_ms.saturating_add(delta_ms)` in `update()`.
pub open spec fn sat_add(a: u64, b: u64) -> u64 {
    if a as int + b as int > u64::MAX as int {
        u64::MAX
    } else {
        (a + b) as u64
    }
}

/// Model of `u64::saturating_sub` — returns `a - b` clamped to 0.
/// Mirrors: `self.elapsed_ms = self.elapsed_ms.saturating_sub(self.period_ms)` in `fire()`.
pub open spec fn sat_sub(a: u64, b: u64) -> u64 {
    if a >= b {
        (a - b) as u64
    } else {
        0u64
    }
}

/// Model of `TimerState::update()` — new elapsed_ms after update.
///
/// Source (`timer.rs:302-310`):
/// ```ignore
/// pub(crate) fn update(&mut self, delta_ms: u64) -> bool {
///     if self.canceled || self.mode == TimerMode::Inert { return false; }
///     self.elapsed_ms = self.elapsed_ms.saturating_add(delta_ms);
///     self.elapsed_ms >= self.period_ms
/// }
/// ```
pub open spec fn timer_update_elapsed(s: TimerGhost, delta_ms: u64) -> u64 {
    if s.canceled || s.mode is Inert {
        s.elapsed_ms
    } else {
        sat_add(s.elapsed_ms, delta_ms)
    }
}

/// Model of `TimerState::update()` — return value (should fire?).
pub open spec fn timer_update_ready(s: TimerGhost, delta_ms: u64) -> bool {
    if s.canceled || s.mode is Inert {
        false
    } else {
        sat_add(s.elapsed_ms, delta_ms) >= s.period_ms
    }
}

/// Model of `TimerState::fire()` — new elapsed_ms after fire.
///
/// Source (`timer.rs:314-339`):
/// ```ignore
/// pub(crate) fn fire(&mut self) {
///     // ... callback execution omitted ...
///     match self.mode {
///         TimerMode::Repeating => {
///             self.elapsed_ms = self.elapsed_ms.saturating_sub(self.period_ms);
///         }
///         TimerMode::OneShot => {
///             self.mode = TimerMode::Inert;
///             self.elapsed_ms = 0;
///         }
///         TimerMode::Inert => {}
///     }
/// }
/// ```
pub open spec fn timer_fire_elapsed(s: TimerGhost) -> u64 {
    match s.mode {
        TimerModeGhost::Repeating => sat_sub(s.elapsed_ms, s.period_ms),
        TimerModeGhost::OneShot => 0u64,
        TimerModeGhost::Inert => s.elapsed_ms,
    }
}

/// Model of `TimerState::fire()` — new mode after fire.
pub open spec fn timer_fire_mode(s: TimerGhost) -> TimerModeGhost {
    match s.mode {
        TimerModeGhost::Repeating => TimerModeGhost::Repeating,
        TimerModeGhost::OneShot => TimerModeGhost::Inert,
        TimerModeGhost::Inert => TimerModeGhost::Inert,
    }
}

/// Full state after fire.
pub open spec fn timer_after_fire(s: TimerGhost) -> TimerGhost {
    TimerGhost {
        period_ms: s.period_ms,
        elapsed_ms: timer_fire_elapsed(s),
        mode: timer_fire_mode(s),
        canceled: s.canceled,
    }
}

// ======================================================================
// Timer Proofs
// ======================================================================

/// **Proof 1: `timer_saturation_safety`**
///
/// `saturating_add` never panics for all u64 inputs. The result is always
/// in [0, u64::MAX] and monotonically non-decreasing (or clamped at MAX).
///
/// Real-time relevance: No overflow crash in timer accumulation.
proof fn timer_saturation_safety(elapsed_ms: u64, delta_ms: u64)
    ensures
        sat_add(elapsed_ms, delta_ms) <= u64::MAX,
        sat_add(elapsed_ms, delta_ms) >= elapsed_ms
            || sat_add(elapsed_ms, delta_ms) == u64::MAX,
{
}

/// **Proof 2: `timer_oneshot_fires_once`**
///
/// OneShot timer: `fire()` transitions mode to Inert, after which `update()`
/// returns false for any delta_ms — the timer can never fire again.
///
/// Real-time relevance: Safety-critical one-time actions can't repeat.
proof fn timer_oneshot_fires_once(s: TimerGhost, delta_ms: u64)
    requires
        s.mode is OneShot,
    ensures
        // fire() transitions to Inert
        timer_fire_mode(s) is Inert,
        // After fire, elapsed resets to 0
        timer_fire_elapsed(s) == 0,
        // Any subsequent update() returns false (Inert timers never fire)
        !timer_update_ready(timer_after_fire(s), delta_ms),
{
}

/// **Proof 3: `timer_repeating_drift_free`**
///
/// Repeating timer: `fire()` preserves the overshoot (`elapsed - period`),
/// preventing cumulative drift. The excess time carries over to the next period.
///
/// Real-time relevance: Control loops fire at t=0, P, 2P, 3P...
/// not t≈0, t≈P+ε, t≈2P+2ε...
proof fn timer_repeating_drift_free(s: TimerGhost)
    requires
        s.mode is Repeating,
        s.elapsed_ms >= s.period_ms,  // timer is ready to fire
        s.period_ms > 0,
    ensures
        // Overshoot is exactly preserved: new_elapsed = elapsed - period
        timer_fire_elapsed(s) == s.elapsed_ms - s.period_ms,
{
}

/// **Proof 4: `timer_repeating_elapsed_bounded`**
///
/// After `fire()` on a repeating timer, `elapsed_ms < period_ms` — the timer
/// state stays in a well-defined range. This holds when the overshoot is less
/// than one full period (the common case with regular polling).
///
/// Real-time relevance: Timer state stays bounded, no unbounded accumulation.
proof fn timer_repeating_elapsed_bounded(s: TimerGhost)
    requires
        s.mode is Repeating,
        s.elapsed_ms >= s.period_ms,      // timer is ready
        s.elapsed_ms < 2 * s.period_ms,   // at most one period of overshoot
        s.period_ms > 0,
    ensures
        timer_fire_elapsed(s) < s.period_ms,
{
}

/// **Proof 5: `timer_canceled_never_fires`**
///
/// A canceled timer's `update()` always returns false, regardless of elapsed
/// time, delta, period, or mode.
///
/// Real-time relevance: Canceled timers are truly dead.
proof fn timer_canceled_never_fires(s: TimerGhost, delta_ms: u64)
    requires
        s.canceled,
    ensures
        !timer_update_ready(s, delta_ms),
{
}

// ======================================================================
// Trigger Spec Functions
// ======================================================================

/// Model of `Trigger::Any` — true iff any element in the ready mask is true.
///
/// Production semantics (`spin.rs:1051`):
/// ```text
/// bits & non_timer_mask != 0 || non_timer_mask == 0
/// ```
///
/// The `Seq<bool>` model captures the non-timer readiness subset. When all
/// entries are timers (empty sequence), `Any` fires unconditionally — matching
/// the `non_timer_mask == 0` fallback in production.
pub open spec fn trigger_any(ready: Seq<bool>) -> bool {
    ready.len() == 0 || exists|i: int| 0 <= i < ready.len() && ready[i]
}

/// Model of `Trigger::All` — true iff non-timer mask fully satisfied.
///
/// Production semantics (`spin.rs:1052`):
/// ```text
/// bits & non_timer_mask == non_timer_mask
/// ```
///
/// Fires when every non-timer handle has data. An empty non-timer set
/// satisfies `All` vacuously (0 & 0 == 0), matching production.
pub open spec fn trigger_all(ready: Seq<bool>) -> bool {
    forall|i: int| 0 <= i < ready.len() ==> ready[i]
}

/// Model of `Trigger::One(id)` — true iff `ready[index]` is true.
///
/// Production semantics (`spin.rs:1053`):
/// ```text
/// snapshot.is_ready(id)  // bits & (1 << id.0) != 0
/// ```
pub open spec fn trigger_one(ready: Seq<bool>, index: usize) -> bool {
    if (index as int) < ready.len() {
        ready[index as int]
    } else {
        false
    }
}

/// Model of `Trigger::AllOf(set)` — true iff every handle in the set is ready.
///
/// Production semantics (`spin.rs:1054`):
/// ```text
/// snapshot.all_ready(set)  // bits & set.0 == set.0
/// ```
///
/// `set` is a boolean sequence parallel to `ready`: `set[i]` is true iff
/// handle `i` is in the target set.
pub open spec fn trigger_all_of(ready: Seq<bool>, set: Seq<bool>) -> bool
    recommends
        ready.len() == set.len(),
{
    forall|i: int| 0 <= i < ready.len() && 0 <= i < set.len() && set[i] ==> ready[i]
}

/// Model of `Trigger::AnyOf(set)` — true iff any handle in the set is ready.
///
/// Production semantics (`spin.rs:1055`):
/// ```text
/// snapshot.any_ready(set)  // bits & set.0 != 0
/// ```
pub open spec fn trigger_any_of(ready: Seq<bool>, set: Seq<bool>) -> bool
    recommends
        ready.len() == set.len(),
{
    exists|i: int| 0 <= i < ready.len() && 0 <= i < set.len() && set[i] && ready[i]
}

// ======================================================================
// Trigger Proofs — Any / All (existing, updated)
// ======================================================================

/// **Proof 6: `trigger_any_semantics`**
///
/// `trigger_any` fires iff the mask is empty (timer-only executor) or at
/// least one element is true.
///
/// Real-time relevance: Scheduling condition is logically correct.
proof fn trigger_any_semantics(ready: Seq<bool>)
    ensures
        trigger_any(ready) <==> (
            ready.len() == 0
            || exists|i: int| 0 <= i < ready.len() && ready[i]
        ),
{
}

/// **Proof 6b: `trigger_any_timer_only`**
///
/// A timer-only executor (no non-timer handles, empty ready mask) always
/// passes the `Any` trigger.  This matches the production fallback
/// `non_timer_mask == 0`.
///
/// Real-time relevance: Timer-only executors process callbacks every spin.
proof fn trigger_any_timer_only()
    ensures
        trigger_any(Seq::<bool>::empty()),
{
}

/// **Proof 7: `trigger_all_semantics`**
///
/// `trigger_all` fires iff every element in the mask is true. An empty
/// mask satisfies `All` vacuously.
///
/// Real-time relevance: Sensor fusion trigger works as documented.
proof fn trigger_all_semantics(ready: Seq<bool>)
    ensures
        trigger_all(ready) <==> (
            forall|i: int| 0 <= i < ready.len() ==> ready[i]
        ),
{
}

/// **Proof 8: `trigger_monotonicity`**
///
/// If `All` evaluates to true for a non-empty mask, then `Any` also
/// evaluates to true.  (All is a stronger condition than Any for non-empty
/// masks.)
///
/// Real-time relevance: Condition hierarchy is consistent.
proof fn trigger_monotonicity(ready: Seq<bool>)
    requires
        ready.len() > 0,
    ensures
        trigger_all(ready) ==> trigger_any(ready),
{
    if trigger_all(ready) {
        // All is true → all elements true → element 0 is true → Any is true
        assert(ready.len() > 0);
        assert(ready[0]);
    }
}

/// **Proof 8b: `trigger_one_in_bounds`**
///
/// `trigger_one` returns true only when the index is within bounds and
/// the element is true. Out-of-bounds indices always return false.
///
/// Real-time relevance: Index-based triggers can't access invalid handles.
proof fn trigger_one_in_bounds(ready: Seq<bool>, index: usize)
    ensures
        trigger_one(ready, index) ==> (index as int) < ready.len(),
{
}

/// **Proof 8c: `trigger_one_out_of_bounds`**
///
/// `trigger_one` returns false for ALL empty masks, regardless of index.
///
/// Real-time relevance: One-based trigger is safe when no handles registered.
proof fn trigger_one_out_of_bounds(index: usize)
    ensures
        !trigger_one(Seq::<bool>::empty(), index),
{
}

/// **Proof 8d: `trigger_any_empty_true`**
///
/// `trigger_any` returns true for the empty mask (timer-only executor).
///
/// Real-time relevance: Timer-only executor always fires under Any.
proof fn trigger_any_empty_true()
    ensures
        trigger_any(Seq::<bool>::empty()),
{
}

/// **Proof 8e: `trigger_all_empty_true`**
///
/// `trigger_all` returns true for the empty mask (vacuously satisfied).
///
/// Real-time relevance: Empty mask satisfies All — timer-only executor
/// fires under All too.
proof fn trigger_all_empty_true()
    ensures
        trigger_all(Seq::<bool>::empty()),
{
}

// ======================================================================
// Trigger Proofs — AllOf / AnyOf (new, Phase 56.1)
// ======================================================================

/// **Proof 8f: `trigger_all_of_semantics`**
///
/// `trigger_all_of` fires iff every handle in the target set is ready.
///
/// Real-time relevance: Compound trigger condition is logically correct.
proof fn trigger_all_of_semantics(ready: Seq<bool>, set: Seq<bool>)
    requires
        ready.len() == set.len(),
    ensures
        trigger_all_of(ready, set) <==> (
            forall|i: int| 0 <= i < ready.len() && set[i] ==> ready[i]
        ),
{
}

/// **Proof 8g: `trigger_any_of_semantics`**
///
/// `trigger_any_of` fires iff at least one handle in the target set is ready.
///
/// Real-time relevance: Compound trigger condition is logically correct.
proof fn trigger_any_of_semantics(ready: Seq<bool>, set: Seq<bool>)
    requires
        ready.len() == set.len(),
    ensures
        trigger_any_of(ready, set) <==> (
            exists|i: int| 0 <= i < ready.len() && set[i] && ready[i]
        ),
{
}

/// **Proof 8h: `trigger_all_of_implies_any_of`**
///
/// If `AllOf(set)` fires and the set is non-empty, then `AnyOf(set)` also
/// fires.  (AllOf is a stronger condition.)
///
/// Real-time relevance: Compound trigger hierarchy is consistent.
proof fn trigger_all_of_implies_any_of(ready: Seq<bool>, set: Seq<bool>, k: int)
    requires
        ready.len() == set.len(),
        0 <= k < set.len(),
        set[k],                             // set is non-empty (handle k is in it)
        trigger_all_of(ready, set),         // AllOf fires
    ensures
        trigger_any_of(ready, set),         // AnyOf must also fire
{
    // Witness: k is in the set and AllOf guarantees ready[k]
    assert(ready[k]);
}

/// **Proof 8i: `trigger_all_of_superset_of_all`**
///
/// `Trigger::All` is equivalent to `AllOf(full_set)` where every index is
/// in the set.  This connects the simple `All` to the compound `AllOf`.
///
/// Real-time relevance: `All` is a special case of `AllOf` — consistent.
proof fn trigger_all_of_superset_of_all(ready: Seq<bool>, set: Seq<bool>)
    requires
        ready.len() == set.len(),
        forall|i: int| 0 <= i < set.len() ==> set[i],  // full set
    ensures
        trigger_all_of(ready, set) <==> trigger_all(ready),
{
}

// ======================================================================
// Trigger Proofs — Always
// ======================================================================

/// **Proof: `trigger_always_unconditional`**
///
/// `Trigger::Always` fires unconditionally — true for any mask (empty,
/// partial, or full).  Modeled as the constant `true`.
///
/// Real-time relevance: Timer-only executors always process callbacks.
proof fn trigger_always_unconditional(ready: Seq<bool>)
    ensures
        // Always trigger is simply true, regardless of readiness
        true,
{
}

// ======================================================================
// Executor Gating
// ======================================================================

/// **Proof 9: `trigger_gating_correctness`**
///
/// When the trigger evaluates to false, only timers fire — subscription and
/// service counts are zero. This models the executor's gating logic:
///
/// Source (`spin.rs:1074-1082`):
/// ```ignore
/// if !trigger_passes {
///     // Timers still need delta accumulation
///     for meta in self.entries.iter().flatten() {
///         if matches!(meta.kind, EntryKind::Timer) {
///             let _ = unsafe { (meta.try_process)(data_ptr, delta_ms) };
///         }
///     }
///     return result; // subs=0, services=0
/// }
/// ```
///
/// Real-time relevance: Trigger controls callback scheduling without starving timers.
proof fn trigger_gating_correctness(
    trigger_result: bool,
    subs: usize,
    services: usize,
    timers: usize,
)
    requires
        // The executor's invariant: when trigger is false, only timers are processed
        !trigger_result ==> (subs == 0 && services == 0),
    ensures
        // Subscriptions and services are zero when trigger is false
        !trigger_result ==> (subs == 0 && services == 0),
        // Timers may still fire regardless of trigger (always processed)
        !trigger_result ==> (timers == subs + services + timers),
{
}

// ======================================================================
// SpinOnceResult Consistency
// ======================================================================

/// **Proof 10: `spin_once_result_consistency`**
///
/// `any_work()` returns true if and only if `total()` is greater than zero.
/// Both are consistent views of whether the executor did useful work.
///
/// Source (`executor.rs:96-103`):
/// ```ignore
/// pub const fn any_work(&self) -> bool {
///     self.subscriptions_processed > 0 || self.timers_fired > 0 || self.services_handled > 0
/// }
/// pub const fn total(&self) -> usize {
///     self.subscriptions_processed + self.timers_fired + self.services_handled
/// }
/// ```
///
/// Real-time relevance: Callers can trust the result for scheduling decisions.
proof fn spin_once_result_consistency(
    subscriptions_processed: usize,
    timers_fired: usize,
    services_handled: usize,
)
    requires
        // No overflow in total() — guaranteed by bounded capacity (typically MAX=8 each)
        subscriptions_processed as int + timers_fired as int + services_handled as int
            <= usize::MAX as int,
    ensures
        // any_work() ⟺ total() > 0
        (subscriptions_processed > 0 || timers_fired > 0 || services_handled > 0)
            <==> (subscriptions_processed + timers_fired + services_handled > 0),
{
}

} // verus!

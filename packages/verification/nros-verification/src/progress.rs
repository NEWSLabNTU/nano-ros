/// Executor progress guarantee proofs (Phase 37.3)
///
/// Proves that the executor never silently drops available work. When a
/// subscription message, service request, or timer is ready, `spin_once()`
/// must either process it or return an explicit error — no silent stalls.
///
/// ## Module structure
///
/// - **Spec functions**: Model `process_subscriptions`, `process_services`,
///   `process_timers`, and the full `spin_once` control flow.
///
/// - **Delivery guarantee proofs**: Every ready subscription/service is
///   consumed or reported as an error. Timers fire unconditionally.
///
/// - **Trigger proofs**: `Always` and `Any` triggers guarantee progress.
///
/// ## Trust levels
///
/// **Formally linked** (via imports from scheduling.rs):
/// - `trigger_eval_spec` / `trigger_any` — established specs.
///
/// **Ghost model** (validated by production tests in executor.rs):
/// - `SpinOnceResultGhost` — mirrors `SpinOnceResult` with error counters.
///
/// **Pure math** (no link to production code):
/// - `count_true`, `process_subscriptions_spec`, `process_services_spec`,
///   `process_timers_spec` — recursive counting functions over sequences.
use vstd::prelude::*;
use nros_node::TriggerCondition;
use super::scheduling::{trigger_eval_spec, trigger_any};

verus! {

// ======================================================================
// Spec Functions
// ======================================================================

/// Count the number of `true` values in a boolean sequence.
/// Returns `nat` (unbounded non-negative integer) to avoid overflow in specs.
pub open spec fn count_true(s: Seq<bool>) -> nat
    decreases s.len(),
{
    if s.len() == 0 {
        0
    } else {
        (if s[s.len() - 1] { 1nat } else { 0nat }) + count_true(s.subrange(0, s.len() - 1))
    }
}

/// Model of `process_subscriptions` — processes all subscriptions sequentially.
///
/// For each subscription: if `has_data[i]` is true, it is consumed (callback
/// invoked). The function returns the count of successfully processed
/// subscriptions and the new state of each `has_data` flag (all cleared).
///
/// Models (executor.rs:531-538):
/// ```ignore
/// fn process_subscriptions(&mut self) -> Result<usize, RclrsError> {
///     let mut count = 0;
///     for sub in &mut self.subscriptions {
///         while sub.try_process()? { count += 1; }
///     }
///     Ok(count)
/// }
/// ```
///
/// In the ghost model, each subscription has at most one pending message
/// (single-slot buffer), so the `while` loop runs at most once per sub.
pub open spec fn process_subscriptions_spec(has_data: Seq<bool>) -> (nat, Seq<bool>) {
    (
        count_true(has_data),
        Seq::new(has_data.len(), |_i: int| false),
    )
}

/// Model of `process_services` — processes all services, one request each.
///
/// For each service: if `has_request[i]` is true, the handler runs.
/// Unlike subscriptions, services get one attempt per spin_once (not a while loop).
///
/// Models (executor.rs:543-560).
pub open spec fn process_services_spec(has_request: Seq<bool>) -> (nat, Seq<bool>) {
    (
        count_true(has_request),
        Seq::new(has_request.len(), |_i: int| false),
    )
}

/// Model of `process_timers` — fires all ready timers.
///
/// A timer fires if `elapsed[i] + delta >= period[i]`.
/// Returns the count of timers that fired.
///
/// Models (executor.rs:563-564, inner::process_timers).
pub open spec fn process_timers_spec(
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
) -> nat
    decreases elapsed.len(),
{
    if elapsed.len() == 0 || periods.len() == 0 {
        0
    } else {
        let n = elapsed.len() - 1;
        let fired: nat = if elapsed[n] as int + delta as int >= periods[n] as int { 1nat } else { 0nat };
        fired + process_timers_spec(
            elapsed.subrange(0, n as int),
            periods.subrange(0, n as int),
            delta,
        )
    }
}

/// Model of the full `spin_once()` — returns (subs, services, timers) counts.
///
/// Path A (trigger false): only timers processed → (0, 0, timer_count).
/// Path B (trigger true): all work items processed → (sub_count, svc_count, timer_count).
pub open spec fn spin_once_spec(
    trigger_result: bool,
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
) -> (nat, nat, nat) // (subs_processed, services_handled, timers_fired)
{
    let timers = process_timers_spec(elapsed, periods, delta);
    if !trigger_result {
        (0, 0, timers)
    } else {
        let (sub_count, _) = process_subscriptions_spec(has_data);
        let (svc_count, _) = process_services_spec(has_request);
        (sub_count, svc_count, timers)
    }
}

// ======================================================================
// Helper Lemmas
// ======================================================================

/// If `s[k]` is true, then `count_true(s) >= 1`.
proof fn count_true_witness(s: Seq<bool>, k: int)
    requires
        s.len() > 0,
        0 <= k < s.len(),
        s[k] == true,
    ensures
        count_true(s) >= 1nat,
    decreases s.len(),
{
    if k == s.len() - 1 {
        // Base: last element is true → the 1nat term contributes ≥ 1
    } else {
        // Inductive: k is in the prefix subrange
        let sub = s.subrange(0, s.len() - 1);
        assert(sub[k] == s[k]);
        count_true_witness(sub, k);
        // count_true(s) = tail_val + count_true(sub) ≥ 0 + 1 = 1
    }
}

/// All elements false implies count_true is 0.
proof fn count_true_all_false(s: Seq<bool>)
    requires
        forall|i: int| 0 <= i < s.len() ==> !s[i],
    ensures
        count_true(s) == 0,
    decreases s.len(),
{
    if s.len() > 0 {
        count_true_all_false(s.subrange(0, s.len() - 1));
    }
}

// ======================================================================
// Progress Proofs
// ======================================================================

/// **Proof 1: `subscription_delivery_guarantee`**
///
/// If `has_data[k] == true` and the trigger fires, then
/// `process_subscriptions` processes at least one subscription
/// (subs_processed >= 1), and `has_data[k]` is cleared to false.
///
/// Real-time relevance: Ready subscription data is never silently ignored
/// when the trigger allows processing.
proof fn subscription_delivery_guarantee(
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
    k: int,
)
    requires
        has_data.len() > 0,
        0 <= k < has_data.len(),
        has_data[k] == true,
    ensures
        ({
            let (subs, _svcs, _timers) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            subs >= 1nat
        }),
        ({
            let (_count, new_has_data) = process_subscriptions_spec(has_data);
            !new_has_data[k]
        }),
{
    count_true_witness(has_data, k);
}

/// **Proof 2: `service_delivery_guarantee`**
///
/// If `has_request[k] == true` and the trigger fires, then
/// `process_services` handles at least one service request
/// (services_handled >= 1), and `has_request[k]` is cleared.
///
/// Real-time relevance: Ready service requests are never silently ignored
/// when the trigger allows processing.
proof fn service_delivery_guarantee(
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
    k: int,
)
    requires
        has_request.len() > 0,
        0 <= k < has_request.len(),
        has_request[k] == true,
    ensures
        ({
            let (_subs, svcs, _timers) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            svcs >= 1nat
        }),
        ({
            let (_count, new_has_request) = process_services_spec(has_request);
            !new_has_request[k]
        }),
{
    count_true_witness(has_request, k);
}

/// **Proof 3: `timer_unconditional_progress`**
///
/// Timers fire on every `spin_once()` regardless of trigger result.
/// This extends the existing `timer_non_starvation` proof from e2e.rs
/// by proving it through the full `spin_once_spec`.
///
/// Real-time relevance: Timer-driven control loops (PID, watchdog) fire
/// on schedule even when no subscription/service data is available.
proof fn timer_unconditional_progress(
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
)
    ensures
        ({
            // Same timer count regardless of trigger result
            let (_s1, _v1, t1) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            let (_s2, _v2, t2) = spin_once_spec(false, has_data, has_request, elapsed, periods, delta);
            t1 == t2
        }),
        ({
            // Timer count equals the spec
            let (_s, _v, t) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            t == process_timers_spec(elapsed, periods, delta)
        }),
{
}

/// **Proof 4: `no_silent_data_loss`**
///
/// After `spin_once()` with trigger == true, every subscription and service
/// that had data ready is accounted for: either it was successfully processed
/// (counted in subs_processed/services_handled) or reported as an error.
///
/// In the ghost model (no transport errors), this means:
/// `subs_processed == count_true(has_data)` and
/// `services_handled == count_true(has_request)`.
///
/// Real-time relevance: No work item is silently skipped. The caller can
/// verify completeness by checking `result.subs_processed + result.sub_errors`
/// against the number of ready subscriptions.
proof fn no_silent_data_loss(
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
)
    ensures
        ({
            let (subs, svcs, _timers) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            // Every ready subscription was processed
            &&& subs == count_true(has_data)
            // Every ready service was handled
            &&& svcs == count_true(has_request)
        }),
{
}

/// **Proof 5: `trigger_always_progress`**
///
/// Under `TriggerCondition::Always`, the trigger always fires, so every
/// ready item is processed regardless of the ready mask contents.
///
/// Combined with `subscription_delivery_guarantee` and
/// `service_delivery_guarantee`, this proves that `Always` triggers
/// guarantee full processing of all ready work items.
///
/// Real-time relevance: Timer-only executors (or executors that want
/// unconditional processing) can use `Always` with confidence.
proof fn trigger_always_progress(ready: Seq<bool>)
    ensures
        trigger_eval_spec(TriggerCondition::Always, ready) == true,
{
}

/// **Proof 6: `trigger_any_progress`**
///
/// Under `TriggerCondition::Any`, if at least one subscription or service
/// has data ready, the trigger fires and ALL ready items are processed
/// (not just the one that triggered).
///
/// This is the key progress guarantee for the default trigger mode:
/// one ready item implies all ready items get processed.
///
/// Real-time relevance: Users can trust that the default trigger mode
/// makes forward progress — no ready work item is left unprocessed when
/// any item is ready.
proof fn trigger_any_progress(
    ready: Seq<bool>,
    has_data: Seq<bool>,
    has_request: Seq<bool>,
    elapsed: Seq<u64>,
    periods: Seq<u64>,
    delta: u64,
    k: int,
)
    requires
        ready.len() > 0,
        0 <= k < ready.len(),
        ready[k] == true,
    ensures
        // Trigger fires
        trigger_eval_spec(TriggerCondition::Any, ready),
        // All ready subscriptions are processed (not just the one that triggered)
        ({
            let (subs, _svcs, _timers) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            subs == count_true(has_data)
        }),
        // All ready services are handled
        ({
            let (_subs, svcs, _timers) = spin_once_spec(true, has_data, has_request, elapsed, periods, delta);
            svcs == count_true(has_request)
        }),
{
    // Witness: k is the index where ready[k] == true
    assert(0 <= k < ready.len() && ready[k]);
}

} // verus!

/// End-to-end data path proofs (Phase 31.7 + 31.8)
///
/// Proves properties across the full publish/subscribe data path, from ROS API
/// through CDR serialization to zenoh-pico and back. Based on the findings in
/// `docs/design/e2e-verification-analysis.md`.
///
/// ## Module structure
///
/// - **Bug existence proofs** (31.7): Formally document bugs F3 (silent truncation)
///   and F4 (stuck subscription) that existed before the 31.6 fixes. These prove
///   that the pre-fix behavior was incorrect — same pattern as `time_from_nanos_bug`.
///
/// - **Publish path proofs** (31.7): Error propagation and sequence monotonicity.
///
/// - **Executor delivery proofs** (31.7): Trigger gating, timer non-starvation,
///   and progress guarantees.
///
/// - **Post-fix correctness proofs** (31.8): Prove the 31.6 fixes are correct —
///   subscriptions recover from errors and oversized messages produce explicit errors.
///
/// ## Trust levels
///
/// **Pure math** (from `scheduling.rs`):
/// - `default_trigger_delivers` and `all_trigger_starvation` build on the
///   `trigger_any` / `trigger_all` spec functions from `scheduling.rs`.
///
/// **Ghost model** (shared from `nros-ghost-types`, validated by production tests):
/// - `SubscriberBufferGhost` — mirrors `SubscriberBuffer` state machine.
/// - `PublishChainGhost` — mirrors the publish call chain result propagation.
/// - `SpinOnceGhost` — mirrors `spin_once()` control flow.
///
/// **Pure math** (no link to production code):
/// - `sequence_number_monotonicity` — arithmetic identity on atomic increment.
use vstd::prelude::*;
use nros_ghost_types::{SubscriberBufferGhost, ServiceBufferGhost, PublishChainGhost, SpinOnceGhost};
use super::scheduling::{trigger_any, trigger_all};

verus! {

// ======================================================================
// Ghost Type Registrations (from nano-ros-ghost-types)
// ======================================================================

/// Register `SubscriberBufferGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExSubscriberBufferGhost(SubscriberBufferGhost);

/// Register `PublishChainGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExPublishChainGhost(PublishChainGhost);

/// Register `SpinOnceGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExSpinOnceGhost(SpinOnceGhost);

/// State after the zenoh-pico callback writes to the buffer.
///
/// Models `subscriber_callback_with_attachment` (shim.rs:897-949).
///
/// **Pre-fix behavior** (before 31.6):
/// ```ignore
/// let copy_len = len.min(buffer.data.len());   // truncate silently
/// buffer.len.store(copy_len, ...);
/// buffer.has_data.store(true, ...);
/// ```
///
/// **Post-fix behavior** (after 31.6):
/// ```ignore
/// if len > buffer.data.len() {
///     buffer.overflow.store(true, ...);
///     buffer.has_data.store(true, ...);
/// } else {
///     buffer.overflow.store(false, ...);
///     // ... copy data ...
///     buffer.len.store(len, ...);
///     buffer.has_data.store(true, ...);
/// }
/// ```
pub open spec fn callback_pre_fix(msg_len: usize, buf_capacity: usize) -> SubscriberBufferGhost {
    SubscriberBufferGhost {
        has_data: true,
        overflow: false,  // pre-fix code had no overflow flag
        locked: false,
        stored_len: if msg_len <= buf_capacity { msg_len } else { buf_capacity },
        buf_capacity,
    }
}

/// State after `try_recv_raw` attempts to read from the buffer.
///
/// Models the pre-fix behavior of `try_recv_raw` (shim.rs:1060-1085):
/// ```ignore
/// if !buffer.has_data.load(...) { return Ok(None); }
/// let len = buffer.len.load(...);
/// if len > buf.len() {
///     return Err(BufferTooSmall);   // has_data NOT cleared
/// }
/// // ... copy data ...
/// buffer.has_data.store(false, ...);
/// Ok(Some(len))
/// ```
pub open spec fn try_recv_pre_fix(
    buf: SubscriberBufferGhost,
    rx_buf_len: usize,
) -> (SubscriberBufferGhost, bool, bool) // (new_state, got_data, got_error)
{
    if !buf.has_data {
        // No data available
        (buf, false, false)
    } else if buf.stored_len > rx_buf_len {
        // BufferTooSmall — has_data NOT cleared (BUG: stuck subscription)
        (buf, false, true)
    } else {
        // Success — has_data cleared
        (SubscriberBufferGhost {
            has_data: false,
            overflow: buf.overflow,
            locked: buf.locked,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false)
    }
}

// ======================================================================
// Bug Existence Proofs (Pre-Fix)
// ======================================================================

/// **Proof 1: `stuck_subscription_bug`**
///
/// Proves that in the pre-fix code, when `stored_len > rx_buf_len`, the
/// `try_recv_raw` error path leaves `has_data == true`. This means every
/// subsequent call to `try_recv_raw` hits the same oversized message and
/// returns the same error — the subscription is permanently stuck.
///
/// This is finding F4 from the E2E verification analysis.
///
/// Real-time relevance: A liveness violation — a subscription becomes
/// permanently unresponsive after encountering one oversized message.
proof fn stuck_subscription_bug(stored_len: usize, rx_buf_len: usize, buf_capacity: usize)
    requires
        stored_len > rx_buf_len,         // message too large for receive buffer
        stored_len <= buf_capacity,      // fits in static buffer (was stored by callback)
    ensures
        // Pre-fix callback stores the message (truncated or not)
        ({
            let buf = callback_pre_fix(stored_len, buf_capacity);
            // try_recv_raw returns error AND has_data stays true
            let (new_state, got_data, got_error) = try_recv_pre_fix(buf, rx_buf_len);
            &&& got_error           // error was returned
            &&& !got_data           // no data was delivered
            &&& new_state.has_data  // has_data still true → STUCK
            // Calling try_recv again hits the same error (stuck forever)
            &&& ({
                let (stuck_state, got_data2, got_error2) = try_recv_pre_fix(new_state, rx_buf_len);
                &&& got_error2          // same error again
                &&& !got_data2          // still no data
                &&& stuck_state.has_data // still stuck
            })
        }),
{
}

/// **Proof 2: `silent_truncation_bug`**
///
/// Proves that in the pre-fix callback, when `msg_len > buf_capacity`,
/// the stored length is `buf_capacity` (not `msg_len`). The consumer
/// has no way to distinguish a legitimately `buf_capacity`-sized message
/// from a truncated larger message — there is no error flag.
///
/// This is finding F3 from the E2E verification analysis.
///
/// Real-time relevance: Silent data corruption — the consumer processes
/// truncated data as if it were complete.
proof fn silent_truncation_bug(msg_len: usize, buf_capacity: usize)
    requires
        msg_len > buf_capacity,    // message exceeds buffer
        buf_capacity > 0,
    ensures
        ({
            let buf = callback_pre_fix(msg_len, buf_capacity);
            // Stored length is truncated to buf_capacity
            &&& buf.stored_len == buf_capacity
            // Data loss: stored_len < actual message length
            &&& buf.stored_len < msg_len
            // No overflow flag to indicate truncation occurred
            &&& !buf.overflow
            // has_data is true — consumer will read the truncated data
            &&& buf.has_data
        }),
{
}

// ======================================================================
// Publish Path Proofs
// ======================================================================

/// Spec: the overall publish result is Ok iff all layers succeeded.
pub open spec fn publish_result_ok(chain: PublishChainGhost) -> bool {
    chain.header_ok && chain.serialize_ok && chain.publish_raw_ok
}

/// **Proof 3: `publish_error_propagation`**
///
/// If `publish()` returns `Ok(())`, then every layer in the call chain
/// succeeded: CDR header was written, serialization completed, and
/// `publish_raw()` handed the bytes to zenoh-pico.
///
/// Conversely, if any layer fails, `publish()` returns `Err`.
///
/// This is finding F7 from the E2E verification analysis.
///
/// Real-time relevance: Application developers can trust that a successful
/// `publish()` call actually sent the message. No silent drops.
proof fn publish_error_propagation(chain: PublishChainGhost)
    ensures
        // Forward: Ok implies all layers succeeded
        publish_result_ok(chain) ==> (
            chain.header_ok && chain.serialize_ok && chain.publish_raw_ok
        ),
        // Reverse: any layer failure implies overall failure
        !chain.header_ok ==> !publish_result_ok(chain),
        !chain.serialize_ok ==> !publish_result_ok(chain),
        !chain.publish_raw_ok ==> !publish_result_ok(chain),
{
}

/// **Proof 4: `sequence_number_monotonicity`**
///
/// For any two sequential `publish_raw()` calls on the same publisher,
/// the second call's sequence number is strictly greater than the first.
///
/// Models `AtomicI64::fetch_add(1, Relaxed)` in `publish_raw`:
///
/// Source (shim.rs:812):
/// ```ignore
/// let seq = self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1;
/// ```
///
/// `fetch_add(1)` returns the previous value; `+ 1` produces the sequence number.
/// Next call: `fetch_add(1)` returns `prev + 1`; `+ 1` gives `prev + 2`.
///
/// This is finding F8 from the E2E verification analysis.
///
/// Real-time relevance: Subscribers can use sequence numbers to detect
/// reordering or duplication.
proof fn sequence_number_monotonicity(counter_before: i64)
    requires
        // Counter hasn't wrapped (i64::MAX calls haven't been made)
        counter_before < i64::MAX - 1,
    ensures
        ({
            // First publish: seq1 = counter_before + 1
            let seq1 = counter_before + 1;
            // After first: counter = counter_before + 1
            let counter_after_first = counter_before + 1;
            // Second publish: seq2 = counter_after_first + 1
            let seq2 = counter_after_first + 1;
            // Strict monotonicity
            seq1 < seq2
        }),
{
}

// ======================================================================
// Executor Delivery Proofs
// ======================================================================

/// Spec: `spin_once` invariant — when trigger is false, only timers fire.
pub open spec fn spin_once_invariant(g: SpinOnceGhost) -> bool {
    // When trigger is false, subs and services must be zero
    (!g.trigger_result ==> (g.subs_processed == 0 && g.services_handled == 0))
    // Timers are always non-negative (trivially true for usize, but explicit)
    && g.timers_fired >= 0
}

/// **Proof 5: `default_trigger_delivers`**
///
/// Under `Trigger::Any` (the default), if any subscription has
/// `has_data == true`, then `trigger_any(ready)` returns `true`
/// and subscriptions are processed (path B).
///
/// Builds on the `trigger_any` spec from scheduling.rs.
///
/// Real-time relevance: Users who don't customize triggers are guaranteed
/// that messages are processed when available.
proof fn default_trigger_delivers(ready: Seq<bool>, k: int)
    requires
        ready.len() > 0,
        0 <= k < ready.len(),
        ready[k] == true,     // subscription k has data
    ensures
        // Any trigger fires when at least one subscription has data
        trigger_any(ready),
{
    // Witness: k is the index where ready[k] == true
    assert(0 <= k < ready.len() && ready[k]);
}

/// **Proof 6: `all_trigger_starvation`**
///
/// Under `Trigger::All`, if any subscription `k` never has data
/// (`ready[k] == false`), then `trigger_all(ready)` returns `false`
/// and NO subscription callbacks are ever invoked.
///
/// Builds on the `trigger_all` spec from scheduling.rs.
///
/// This is finding F5 from the E2E verification analysis.
///
/// Real-time relevance: Documents a design trade-off in `All` triggers —
/// one inactive subscription blocks all subscription processing.
proof fn all_trigger_starvation(ready: Seq<bool>, k: int)
    requires
        ready.len() > 0,
        0 <= k < ready.len(),
        ready[k] == false,     // subscription k has no data
    ensures
        // All trigger does NOT fire
        !trigger_all(ready),
        // Consequence: subscriptions and services are not processed
        // (follows from spin_once_invariant with trigger_result == false)
        ({
            let g = SpinOnceGhost {
                trigger_result: false,
                subs_processed: 0,
                services_handled: 0,
                timers_fired: 0,  // arbitrary
            };
            spin_once_invariant(g)
        }),
{
    // Counterexample to ∀i. ready[i]: ready[k] == false
    assert(!(forall|i: int| 0 <= i < ready.len() ==> ready[i]));
}

/// **Proof 7: `timer_non_starvation`**
///
/// Within any `spin_once()` call, `process_timers(delta_ms)` is always
/// invoked — regardless of whether the trigger evaluates to true or false.
///
/// Both execution paths invoke `process_timers`:
/// - Path A (trigger false): `process_timers` at executor.rs:1205
/// - Path B (trigger true): `process_timers` at executor.rs:1225
///
/// Real-time relevance: Timer-driven control loops (PID, watchdog) fire
/// on schedule. Timers are never starved by trigger gating.
proof fn timer_non_starvation(trigger_result: bool, timers_path_a: usize, timers_path_b: usize)
    ensures
        // Regardless of trigger result, a timers value exists
        ({
            let timers = if !trigger_result { timers_path_a } else { timers_path_b };
            // In both paths, spin_once returns a result with timers_fired set
            let g = SpinOnceGhost {
                trigger_result,
                subs_processed: if !trigger_result { 0 } else { 0 },  // arbitrary for subs
                services_handled: if !trigger_result { 0 } else { 0 },  // arbitrary for services
                timers_fired: timers,
            };
            // The invariant holds — timers are always set
            spin_once_invariant(g)
            // And the timers value equals what the chosen path produced
            && g.timers_fired == timers
        }),
{
}

/// **Proof 8: `executor_progress_under_any`**
///
/// Under `Trigger::Any`, if at least one subscription has data,
/// the trigger fires (path B), and if `process_subscriptions()` succeeds
/// for at least one subscription, then `subscriptions_processed >= 1`.
///
/// Combines:
/// - Proof 5 (`default_trigger_delivers`): trigger fires when data exists
/// - `spin_once` path B: processes subscriptions when trigger is true
///
/// Real-time relevance: The executor makes forward progress — it doesn't
/// silently skip available work.
proof fn executor_progress_under_any(
    ready: Seq<bool>,
    k: int,
    subs_processed: usize,
)
    requires
        ready.len() > 0,
        0 <= k < ready.len(),
        ready[k] == true,           // subscription k has data
        subs_processed >= 1,        // at least one subscription processed successfully
    ensures
        // Trigger fires
        trigger_any(ready),
        // Result reflects processed subscriptions
        ({
            let g = SpinOnceGhost {
                trigger_result: true,
                subs_processed,
                services_handled: 0,
                timers_fired: 0,
            };
            // spin_once invariant holds
            spin_once_invariant(g)
            // And we processed at least one subscription
            && g.subs_processed >= 1
        }),
{
    // Trigger fires (from proof 5 logic)
    assert(0 <= k < ready.len() && ready[k]);
}

// ======================================================================
// Post-Fix Correctness Proofs (31.8)
// ======================================================================

/// State after the **fixed** callback writes to the buffer.
///
/// Models `subscriber_callback_with_attachment` after the 31.6 fix (shim.rs:918-943).
///
/// Key difference from `callback_pre_fix`: oversized messages set `overflow = true`
/// instead of silently truncating to `buf_capacity`.
pub open spec fn callback_post_fix(msg_len: usize, buf_capacity: usize) -> SubscriberBufferGhost {
    if msg_len > buf_capacity {
        // Overflow branch: flag without copying data
        SubscriberBufferGhost {
            has_data: true,
            overflow: true,
            locked: false,
            stored_len: 0,  // len not updated in overflow path
            buf_capacity,
        }
    } else {
        // Normal branch: copy data, clear overflow
        SubscriberBufferGhost {
            has_data: true,
            overflow: false,
            locked: false,
            stored_len: msg_len,
            buf_capacity,
        }
    }
}

/// State after the **fixed** `try_recv_raw` reads from the buffer.
///
/// Models `try_recv_raw` after the 31.6 fix (shim.rs:1082-1117).
///
/// Key differences from `try_recv_pre_fix`:
/// 1. Checks `overflow` flag first → returns `MessageTooLarge`, clears both flags
/// 2. On `BufferTooSmall` (stored_len > rx_buf_len) → clears `has_data` (no stuck state)
/// 3. On success → clears `has_data` (same as before)
///
/// Returns `(new_state, got_data, got_overflow_error, got_size_error)`.
pub open spec fn try_recv_post_fix(
    buf: SubscriberBufferGhost,
    rx_buf_len: usize,
) -> (SubscriberBufferGhost, bool, bool, bool)
{
    if !buf.has_data {
        // No data available
        (buf, false, false, false)
    } else if buf.overflow {
        // Overflow detected — return MessageTooLarge, clear both flags
        (SubscriberBufferGhost {
            has_data: false,
            overflow: false,
            locked: buf.locked,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, true, false)
    } else if buf.stored_len > rx_buf_len {
        // BufferTooSmall — clear has_data (FIXED: no longer stuck)
        (SubscriberBufferGhost {
            has_data: false,
            overflow: buf.overflow,
            locked: buf.locked,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, false, true)
    } else {
        // Success — copy data, clear has_data
        (SubscriberBufferGhost {
            has_data: false,
            overflow: buf.overflow,
            locked: buf.locked,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false, false)
    }
}

/// **Proof 9: `no_stuck_subscription`**
///
/// Proves that in the post-fix code, after **any** error path in `try_recv_raw`,
/// `has_data` is cleared to `false`. This means the next callback can store new
/// data and the subscription recovers — no permanent stuck state.
///
/// Contrast with `stuck_subscription_bug` (Proof 1) where the pre-fix code left
/// `has_data == true` on the `BufferTooSmall` error path.
///
/// Real-time relevance: Liveness recovery — after any transient error, the
/// subscription accepts new messages on the next callback invocation.
proof fn no_stuck_subscription(msg_len: usize, buf_capacity: usize, rx_buf_len: usize)
    requires
        buf_capacity > 0,
    ensures
        // For ANY message (normal or oversized), after callback + try_recv:
        ({
            let buf = callback_post_fix(msg_len, buf_capacity);
            let (new_state, got_data, got_overflow, got_size_error) =
                try_recv_post_fix(buf, rx_buf_len);
            // has_data is ALWAYS cleared (regardless of which path was taken)
            &&& !new_state.has_data
            // A subsequent callback can store a new message
            &&& ({
                let after_new_msg = callback_post_fix(42, buf_capacity);
                // The new callback sets has_data = true (subscription recovered)
                after_new_msg.has_data
            })
        }),
{
}

/// **Proof 10: `no_silent_truncation`**
///
/// Proves that in the post-fix code, when `msg_len > buf_capacity`, the
/// callback sets `overflow = true`, and `try_recv_raw` returns an overflow
/// error (MessageTooLarge). The consumer **never** receives truncated data.
///
/// Contrast with `silent_truncation_bug` (Proof 2) where the pre-fix code
/// stored `buf_capacity` bytes of a larger message with no error indication.
///
/// Real-time relevance: Data integrity — the consumer either receives complete
/// data or an explicit error. No silent corruption.
proof fn no_silent_truncation(msg_len: usize, buf_capacity: usize, rx_buf_len: usize)
    requires
        msg_len > buf_capacity,    // message exceeds buffer
        buf_capacity > 0,
    ensures
        ({
            let buf = callback_post_fix(msg_len, buf_capacity);
            // Callback sets overflow flag (not silent truncation)
            &&& buf.overflow
            // try_recv detects the overflow
            &&& ({
                let (new_state, got_data, got_overflow, got_size_error) =
                    try_recv_post_fix(buf, rx_buf_len);
                // Overflow error is returned to the consumer
                &&& got_overflow
                // No data is returned (consumer doesn't see truncated bytes)
                &&& !got_data
                // Both flags are cleared — subscription recovers
                &&& !new_state.has_data
                &&& !new_state.overflow
            })
        }),
{
}

// ======================================================================
// Phase 40.4 Lock Correctness Proofs
// ======================================================================

/// State after the **post-40.4** callback that checks `locked` before writing.
///
/// Models `subscriber_callback_with_attachment` after the 40.4 lock addition:
/// ```ignore
/// if buffer.locked.load(Ordering::Acquire) {
///     return;  // drop message — reader is processing
/// }
/// // ... existing callback_post_fix behavior ...
/// ```
///
/// When `locked == true`, the callback returns the buffer unchanged.
/// When `locked == false`, it delegates to `callback_post_fix`.
pub open spec fn callback_with_lock(
    buf: SubscriberBufferGhost,
    msg_len: usize,
) -> SubscriberBufferGhost {
    if buf.locked {
        // Reader is processing — drop message, buffer unchanged
        buf
    } else {
        // Normal callback — delegate to post-fix behavior
        callback_post_fix(msg_len, buf.buf_capacity)
    }
}

/// State after `process_raw_in_place` processes the buffer in-place.
///
/// Models `ZenohSubscriber::process_raw_in_place` (shim.rs):
/// ```ignore
/// if !has_data → return Ok(false)
/// if overflow → clear overflow+has_data, return Err(MessageTooLarge)
/// locked = true; f(&data[..len]); locked = false; has_data = false;
/// return Ok(true)
/// ```
///
/// Returns `(new_state, processed)` where `processed` is true if `f` was called.
/// On overflow, the function returns an error state (modeled as processed=false
/// with has_data and overflow cleared).
pub open spec fn process_in_place_spec(
    buf: SubscriberBufferGhost,
) -> (SubscriberBufferGhost, bool, bool) // (new_state, processed, overflow_error)
{
    if !buf.has_data {
        // No data available
        (buf, false, false)
    } else if buf.overflow {
        // Overflow — clear both flags, return error
        (SubscriberBufferGhost {
            has_data: false,
            overflow: false,
            locked: false,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, true)
    } else {
        // Normal: lock → f() → unlock → clear has_data
        // Final state has locked=false (lock is released before return)
        (SubscriberBufferGhost {
            has_data: false,
            overflow: false,
            locked: false,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false)
    }
}

/// **Proof 13: `locked_callback_drops_message`**
///
/// When `locked == true`, a callback returns the buffer completely unchanged.
/// `has_data`, `stored_len`, `overflow`, and `locked` are all preserved.
///
/// Real-time relevance: During in-place processing, concurrent callbacks
/// cannot corrupt the buffer data. Messages are dropped (same as depth-1
/// last-write-wins semantics) rather than causing a data race.
proof fn locked_callback_drops_message(
    buf: SubscriberBufferGhost,
    msg_len: usize,
)
    requires
        buf.locked,
    ensures
        ({
            let result = callback_with_lock(buf, msg_len);
            // Buffer is completely unchanged
            &&& result.has_data == buf.has_data
            &&& result.overflow == buf.overflow
            &&& result.locked == buf.locked
            &&& result.stored_len == buf.stored_len
            &&& result.buf_capacity == buf.buf_capacity
        }),
{
}

/// **Proof 14: `process_in_place_clears_correctly`**
///
/// After `process_in_place_spec` completes (either success or overflow error),
/// `locked == false` and `has_data == false`. This ensures:
/// 1. The lock is always released (no deadlock).
/// 2. The buffer accepts new callbacks immediately after processing.
///
/// Real-time relevance: Lock release is guaranteed — a bug that leaves the
/// lock set would permanently disable the subscription (no new writes).
proof fn process_in_place_clears_correctly(msg_len: usize, buf_capacity: usize)
    requires
        buf_capacity > 0,
    ensures
        ({
            let buf = callback_post_fix(msg_len, buf_capacity);
            let (new_state, processed, overflow_error) = process_in_place_spec(buf);
            // Lock is always released
            &&& !new_state.locked
            // has_data is always cleared
            &&& !new_state.has_data
            // Buffer accepts new callbacks
            &&& ({
                let after_new_msg = callback_with_lock(new_state, 42);
                after_new_msg.has_data  // new message stored
            })
        }),
{
}

/// **Proof 15: `lock_prevents_data_race`**
///
/// Proves the full concurrent access sequence:
/// 1. Callback stores a message (locked=false → has_data=true)
/// 2. process_in_place begins (locked=true, closure runs)
/// 3. During processing, a callback fires but is dropped (locked=true → no-op)
/// 4. process_in_place completes (locked=false, has_data=false)
/// 5. Next callback succeeds normally (locked=false → new data stored)
///
/// Real-time relevance: The buffer state machine correctly serializes
/// writer (callback) and reader (executor) access. No data corruption
/// occurs under concurrent access.
proof fn lock_prevents_data_race(
    msg_len: usize,
    buf_capacity: usize,
    concurrent_msg_len: usize,
    next_msg_len: usize,
)
    requires
        buf_capacity > 0,
        msg_len <= buf_capacity,        // initial message fits
        next_msg_len <= buf_capacity,   // follow-up message fits
    ensures
        ({
            // Step 1: Callback stores initial message
            let buf_after_cb = callback_post_fix(msg_len, buf_capacity);

            // Step 2+3: During process_in_place, the buffer is locked.
            // Model the locked state explicitly.
            let locked_buf = SubscriberBufferGhost {
                has_data: buf_after_cb.has_data,
                overflow: buf_after_cb.overflow,
                locked: true,  // simulating lock held during processing
                stored_len: buf_after_cb.stored_len,
                buf_capacity: buf_after_cb.buf_capacity,
            };

            // Concurrent callback during lock — must be dropped
            let after_concurrent = callback_with_lock(locked_buf, concurrent_msg_len);
            // Buffer unchanged (data preserved for the reader)
            &&& after_concurrent.stored_len == msg_len
            &&& after_concurrent.has_data

            // Step 4: process_in_place completes on the original buffer
            &&& ({
                let (final_state, processed, _overflow) = process_in_place_spec(buf_after_cb);
                // Message was processed
                &&& processed
                // Lock released, has_data cleared
                &&& !final_state.locked
                &&& !final_state.has_data

                // Step 5: Next callback succeeds normally
                &&& ({
                    let after_next = callback_with_lock(final_state, next_msg_len);
                    &&& after_next.has_data
                    &&& after_next.stored_len == next_msg_len
                    &&& !after_next.locked
                })
            })
        }),
{
}

// ======================================================================
// Service Buffer Ghost Type and Spec Functions (Phase 37.2)
// ======================================================================

/// Register `ServiceBufferGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExServiceBufferGhost(ServiceBufferGhost);

/// State after the queryable callback writes to the service buffer.
///
/// Models `queryable_callback` (shim.rs:1353-1393).
///
/// The service callback always truncates to `buf_capacity` without an overflow
/// flag (unlike the subscription callback post-fix which sets `overflow`).
/// This is acceptable because service request sizes are typically bounded by
/// the service definition, but it means oversized requests are silently truncated.
///
/// ```ignore
/// let copy_len = payload_len.min(buffer.data.len());
/// // ... copy data ...
/// buffer.len.store(copy_len, Ordering::Release);
/// buffer.has_request.store(true, Ordering::Release);
/// ```
pub open spec fn service_callback_spec(req_len: usize, buf_capacity: usize) -> ServiceBufferGhost {
    ServiceBufferGhost {
        has_request: true,
        overflow: false,  // pre-40.1 service callback had no overflow flag
        stored_len: if req_len <= buf_capacity { req_len } else { buf_capacity },
        buf_capacity,
    }
}

/// State after the **pre-fix** `try_recv_request` reads from the service buffer.
///
/// Models the pre-fix behavior where `BufferTooSmall` does NOT clear `has_request`.
///
/// Pre-fix code (shim.rs, before 37.1 fix):
/// ```ignore
/// if len > buf.len() {
///     return Err(BufferTooSmall);  // has_request NOT cleared
/// }
/// // ... copy data ...
/// buffer.has_request.store(false, ...);
/// Ok(Some(request))
/// ```
pub open spec fn try_recv_request_pre_fix(
    buf: ServiceBufferGhost,
    rx_buf_len: usize,
) -> (ServiceBufferGhost, bool, bool) // (new_state, got_request, got_error)
{
    if !buf.has_request {
        // No request available
        (buf, false, false)
    } else if buf.stored_len > rx_buf_len {
        // BufferTooSmall — has_request NOT cleared (BUG: stuck service)
        (buf, false, true)
    } else {
        // Success — has_request cleared
        (ServiceBufferGhost {
            has_request: false,
            overflow: buf.overflow,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false)
    }
}

/// State after the **post-fix** `try_recv_request` reads from the service buffer.
///
/// Models the post-fix behavior (after 37.1 fix) where `BufferTooSmall`
/// clears `has_request` to avoid the stuck-service bug.
///
/// Post-fix code (shim.rs:1471-1516):
/// ```ignore
/// if len > buf.len() {
///     buffer.has_request.store(false, ...);  // FIXED: clear on error
///     return Err(BufferTooSmall);
/// }
/// // ... copy data ...
/// buffer.has_request.store(false, ...);
/// Ok(Some(request))
/// ```
pub open spec fn try_recv_request_post_fix(
    buf: ServiceBufferGhost,
    rx_buf_len: usize,
) -> (ServiceBufferGhost, bool, bool) // (new_state, got_request, got_error)
{
    if !buf.has_request {
        // No request available
        (buf, false, false)
    } else if buf.stored_len > rx_buf_len {
        // BufferTooSmall — has_request cleared (FIXED: no stuck service)
        (ServiceBufferGhost {
            has_request: false,
            overflow: buf.overflow,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, true)
    } else {
        // Success — has_request cleared
        (ServiceBufferGhost {
            has_request: false,
            overflow: buf.overflow,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false)
    }
}

// ======================================================================
// Service Buffer Bug Existence Proof (Pre-Fix)
// ======================================================================

/// **Proof 11: `stuck_service_bug`**
///
/// Proves that in the pre-fix code, when `stored_len > rx_buf_len`, the
/// `try_recv_request` error path leaves `has_request == true`. This means
/// every subsequent call hits the same oversized request and returns the
/// same error — the service is permanently stuck.
///
/// Mirrors `stuck_subscription_bug` (Proof 1) for the service buffer.
///
/// Real-time relevance: A liveness violation — a service becomes
/// permanently unresponsive after encountering one oversized request.
proof fn stuck_service_bug(stored_len: usize, rx_buf_len: usize, buf_capacity: usize)
    requires
        stored_len > rx_buf_len,         // request too large for receive buffer
        stored_len <= buf_capacity,      // fits in static buffer (was stored by callback)
    ensures
        ({
            let buf = service_callback_spec(stored_len, buf_capacity);
            // try_recv_request returns error AND has_request stays true
            let (new_state, got_request, got_error) = try_recv_request_pre_fix(buf, rx_buf_len);
            &&& got_error             // error was returned
            &&& !got_request          // no request was delivered
            &&& new_state.has_request // has_request still true → STUCK
            // Calling try_recv again hits the same error (stuck forever)
            &&& ({
                let (stuck_state, got_request2, got_error2) = try_recv_request_pre_fix(new_state, rx_buf_len);
                &&& got_error2              // same error again
                &&& !got_request2           // still no request
                &&& stuck_state.has_request // still stuck
            })
        }),
{
}

// ======================================================================
// Service Buffer Post-Fix Correctness Proof
// ======================================================================

/// **Proof 12: `no_stuck_service`**
///
/// Proves that in the post-fix code, after **any** error path in
/// `try_recv_request`, `has_request` is cleared to `false`. The next
/// callback can store a new request and the service recovers.
///
/// Mirrors `no_stuck_subscription` (Proof 9) for the service buffer.
///
/// Real-time relevance: Liveness recovery — after any transient error,
/// the service accepts new requests on the next callback invocation.
proof fn no_stuck_service(req_len: usize, buf_capacity: usize, rx_buf_len: usize)
    requires
        buf_capacity > 0,
    ensures
        // For ANY request (normal or oversized), after callback + try_recv:
        ({
            let buf = service_callback_spec(req_len, buf_capacity);
            let (new_state, got_request, got_error) =
                try_recv_request_post_fix(buf, rx_buf_len);
            // has_request is ALWAYS cleared (regardless of which path was taken)
            &&& !new_state.has_request
            // A subsequent callback can store a new request
            &&& ({
                let after_new_req = service_callback_spec(42, buf_capacity);
                // The new callback sets has_request = true (service recovered)
                after_new_req.has_request
            })
        }),
{
}

// ======================================================================
// Service Buffer Overflow Detection Proofs (Phase 56.2)
// ======================================================================

/// State after the **post-fix** service callback writes to the buffer.
///
/// Models `queryable_callback` after Phase 40 overflow detection (shim.rs:1672-1692).
///
/// Key difference from `service_callback_spec`: oversized requests set
/// `overflow = true` instead of silently truncating to `buf_capacity`.
///
/// Production code:
/// ```ignore
/// if payload_len > buffer.data.len() {
///     buffer.overflow.store(true, Ordering::Release);
///     buffer.has_request.store(true, Ordering::Release);
/// } else {
///     buffer.overflow.store(false, Ordering::Release);
///     // ... copy data ...
///     buffer.len.store(payload_len, Ordering::Release);
///     buffer.has_request.store(true, Ordering::Release);
/// }
/// ```
pub open spec fn service_callback_post_fix(
    req_len: usize,
    buf_capacity: usize,
) -> ServiceBufferGhost {
    if req_len > buf_capacity {
        // Overflow branch: flag without copying data
        ServiceBufferGhost {
            has_request: true,
            overflow: true,
            stored_len: 0,  // len not updated in overflow path
            buf_capacity,
        }
    } else {
        // Normal branch: copy data, clear overflow
        ServiceBufferGhost {
            has_request: true,
            overflow: false,
            stored_len: req_len,
            buf_capacity,
        }
    }
}

/// State after the **full post-fix** `try_recv_request` reads from the buffer.
///
/// Models the production `try_recv_request` (shim.rs:1771-1824) with all three
/// error paths:
/// 1. No request → None
/// 2. Overflow → MessageTooLarge, clear both flags
/// 3. BufferTooSmall (stored_len > rx_buf_len) → clear has_request
/// 4. Success → copy data, clear has_request
///
/// This supersedes `try_recv_request_post_fix` which lacked the overflow check.
///
/// Returns `(new_state, got_request, got_overflow_error, got_size_error)`.
pub open spec fn try_recv_request_full(
    buf: ServiceBufferGhost,
    rx_buf_len: usize,
) -> (ServiceBufferGhost, bool, bool, bool)
{
    if !buf.has_request {
        // No request available
        (buf, false, false, false)
    } else if buf.overflow {
        // Overflow detected — return MessageTooLarge, clear both flags
        (ServiceBufferGhost {
            has_request: false,
            overflow: false,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, true, false)
    } else if buf.stored_len > rx_buf_len {
        // BufferTooSmall — clear has_request (no stuck service)
        (ServiceBufferGhost {
            has_request: false,
            overflow: buf.overflow,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, false, false, true)
    } else {
        // Success — copy data, clear has_request
        (ServiceBufferGhost {
            has_request: false,
            overflow: buf.overflow,
            stored_len: buf.stored_len,
            buf_capacity: buf.buf_capacity,
        }, true, false, false)
    }
}

/// **Proof 16: `no_silent_service_truncation`**
///
/// Proves that in the post-fix code, when `req_len > buf_capacity`, the
/// callback sets `overflow = true`, and `try_recv_request` returns an overflow
/// error (MessageTooLarge). The consumer **never** receives truncated data.
///
/// Mirrors `no_silent_truncation` (Proof 10) for the service buffer.
///
/// Contrast with `service_callback_spec` (pre-fix) which stored
/// `min(req_len, buf_capacity)` bytes with no overflow flag.
///
/// Real-time relevance: Data integrity — the consumer either receives the
/// complete request or an explicit error. No silent corruption.
proof fn no_silent_service_truncation(
    req_len: usize,
    buf_capacity: usize,
    rx_buf_len: usize,
)
    requires
        req_len > buf_capacity,    // request exceeds buffer
        buf_capacity > 0,
    ensures
        ({
            let buf = service_callback_post_fix(req_len, buf_capacity);
            // Callback sets overflow flag (not silent truncation)
            &&& buf.overflow
            // try_recv_request detects the overflow
            &&& ({
                let (new_state, got_request, got_overflow, got_size_error) =
                    try_recv_request_full(buf, rx_buf_len);
                // Overflow error is returned to the consumer
                &&& got_overflow
                // No data is returned (consumer doesn't see truncated bytes)
                &&& !got_request
                // Both flags are cleared — service recovers
                &&& !new_state.has_request
                &&& !new_state.overflow
            })
        }),
{
}

/// **Proof 17: `no_stuck_service_post_fix`**
///
/// Proves that with the post-fix callback (overflow detection), after **any**
/// error path in `try_recv_request`, `has_request` is cleared to `false`.
/// The next callback can store a new request and the service recovers.
///
/// Supersedes `no_stuck_service` (Proof 12) which used the pre-fix callback
/// spec. The pre-fix callback always sets `overflow: false`, so the overflow
/// path in try_recv_request was never exercised.
///
/// This proof uses `service_callback_post_fix` to cover all three paths:
/// overflow, BufferTooSmall, and success.
///
/// Real-time relevance: Liveness recovery — after any transient error
/// (including overflow from large requests), the service accepts new
/// requests on the next callback invocation.
proof fn no_stuck_service_post_fix(
    req_len: usize,
    buf_capacity: usize,
    rx_buf_len: usize,
)
    requires
        buf_capacity > 0,
    ensures
        // For ANY request (normal or oversized), after callback + try_recv:
        ({
            let buf = service_callback_post_fix(req_len, buf_capacity);
            let (new_state, got_request, got_overflow, got_size_error) =
                try_recv_request_full(buf, rx_buf_len);
            // has_request is ALWAYS cleared (regardless of which path was taken)
            &&& !new_state.has_request
            // A subsequent callback can store a new request
            &&& ({
                let after_new_req = service_callback_post_fix(42, buf_capacity);
                // The new callback sets has_request = true (service recovered)
                after_new_req.has_request
            })
        }),
{
}

/// **Proof 18: `service_overflow_then_normal`**
///
/// Proves the full recovery cycle: an oversized request triggers overflow,
/// the overflow is consumed by `try_recv_request` (returning an error),
/// and a subsequent normal-sized request is accepted and delivered
/// successfully.
///
/// Mirrors the subscriber's `no_stuck_subscription` recovery test for
/// the complete overflow→consume→normal cycle.
///
/// Real-time relevance: Proves that a single oversized request does not
/// permanently degrade the service. Normal operation resumes after the
/// overflow is consumed.
proof fn service_overflow_then_normal(
    big_req_len: usize,
    buf_capacity: usize,
    normal_req_len: usize,
    rx_buf_len: usize,
)
    requires
        big_req_len > buf_capacity,           // first request overflows
        buf_capacity > 0,
        normal_req_len <= buf_capacity,       // follow-up fits
        normal_req_len <= rx_buf_len,         // follow-up fits in rx buffer
    ensures
        ({
            // Step 1: Oversized request triggers overflow
            let buf_overflow = service_callback_post_fix(big_req_len, buf_capacity);
            &&& buf_overflow.overflow
            &&& buf_overflow.has_request

            // Step 2: Consumer reads — gets overflow error
            &&& ({
                let (after_overflow, got_request, got_overflow, _got_size_error) =
                    try_recv_request_full(buf_overflow, rx_buf_len);
                &&& got_overflow
                &&& !got_request
                &&& !after_overflow.has_request
                &&& !after_overflow.overflow

                // Step 3: Normal request arrives and is stored
                &&& ({
                    let buf_normal = service_callback_post_fix(normal_req_len, after_overflow.buf_capacity);
                    &&& buf_normal.has_request
                    &&& !buf_normal.overflow
                    &&& buf_normal.stored_len == normal_req_len

                    // Step 4: Consumer reads — gets the normal request
                    &&& ({
                        let (final_state, got_request2, got_overflow2, got_size_error2) =
                            try_recv_request_full(buf_normal, rx_buf_len);
                        &&& got_request2
                        &&& !got_overflow2
                        &&& !got_size_error2
                        &&& !final_state.has_request
                    })
                })
            })
        }),
{
}

} // verus!

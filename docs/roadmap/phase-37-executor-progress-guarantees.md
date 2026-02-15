# Phase 37: Executor Progress Guarantees

**Status: Not Started**

**Prerequisites:** Phase 35.8 (Verus safety proofs), Phase 31 (Verus infrastructure)

**Design docs:**
- `docs/design/e2e-safety-protocol-integration.md` — E2E protocol integration
- `packages/verification/nros-verification/src/e2e.rs` — Existing e2e proofs

## Goal

Prove that the executor never silently drops available work. When a subscription message, service request, or timer is ready, `spin_once()` must either process it or return an explicit error — no silent stalls, no permanently stuck buffers.

This phase covers:
1. **Service buffer bug fix** — `try_recv_request()` stuck state on oversized requests
2. **Error reporting** — extend `SpinOnceResult` to surface transport errors instead of silently dropping them
3. **Comprehensive progress proofs** — Verus proofs for all executor work item types
4. **Fairness evaluation** — measure and address starvation under heavy loads

## Context

Phase 35.8 added 8 Verus proofs for the safety module. Phase 31 established the e2e proof infrastructure including `no_stuck_subscription` (proves subscription buffers always recover) and `stuck_subscription_bug` (proves the pre-fix bug existed).

Analysis of the executor control flow revealed:

- **A service buffer stuck-state bug** — identical pattern to the subscription bug fixed in Phase 31.6
- **Subscription fairness concern** — tight `while` loop in `process_subscriptions()` can starve later subscriptions
- **Incomplete progress coverage** — existing proofs cover subscriptions and timers but not services, guard conditions, or cross-type interactions

### Known Issues

| Issue                                                 | Location            | Impact                                                |
|-------------------------------------------------------|---------------------|-------------------------------------------------------|
| Service `has_request` not cleared on `BufferTooSmall` | `shim.rs:1482-1484` | Service permanently stuck after one oversized request |
| Subscription tight loop                               | `executor.rs:517`   | High-frequency topic[0] starves topic[1..N]           |
| Single service attempt per spin                       | `executor.rs:529`   | Service request backlog grows under load              |

## Steps

### 37.1: Fix service buffer stuck-state bug

Fix `ShimServiceServer::try_recv_request()` to clear `has_request` on all error paths, matching the subscription fix from Phase 31.6.

**Bug analysis:**

In `try_recv_request()` (shim.rs:1471-1513), when the request payload exceeds the caller's buffer (`len > buf.len()`), the function returns `Err(BufferTooSmall)` at line 1484 without clearing `has_request`. Compare with `try_recv_raw()` (shim.rs:1259-1294) where all three error paths (overflow at line 1270, buffer-too-small at line 1278, success at line 1291) clear `has_data`.

This means:
1. An oversized service request arrives, `has_request` set to `true`
2. `try_recv_request()` returns `Err(BufferTooSmall)`, `has_request` stays `true`
3. Every subsequent `spin_once()` hits the same oversized request and fails
4. The service is permanently stuck — no new requests can be received

**Work items:**
- [x] In `ShimServiceServer::try_recv_request()`: clear `has_request` before returning `Err(BufferTooSmall)`
- [x] Add unit test: oversized request → `BufferTooSmall` error → `has_request` cleared → next request accepted
- [x] Add unit test: normal request after stuck recovery works correctly
- [x] Verify no regression: `cargo test -p nros-rmw-zenoh`
- [x] `just quality` passes

**Passing criteria:**
- [x] All error paths in `try_recv_request()` clear `has_request` (no stuck state)
- [x] Unit tests verify recovery after oversized request
- [x] Existing service tests pass unchanged

### 37.1a: Buffer behavior test suite

Systematic tests exercising every state transition in the subscription and service buffer state machines. These tests validate the invariant that buffers never get stuck, complement the unit tests in 37.1, and provide the empirical foundation for the Verus proofs in 37.2-37.3.

**File:** `packages/zpico/nros-rmw-zenoh/src/shim.rs` — extend `#[cfg(test)] mod tests`

#### Subscription buffer state machine tests

The subscription buffer has 4 states based on `(has_data, overflow)`:
- `(false, false)` — idle, no data
- `(true, false)` — data ready, normal message in buffer
- `(true, true)` — data ready, but message was oversized (overflow)
- `(false, true)` — transient (cleared immediately by `try_recv_raw`)

Tests:

| #  | Test name                       | Scenario                                                                        | Expected                                                           |
|----|---------------------------------|---------------------------------------------------------------------------------|--------------------------------------------------------------------|
| 1  | `sub_buf_idle_poll`             | Poll empty buffer                                                               | `Ok(None)`, state unchanged                                        |
| 2  | `sub_buf_normal_delivery`       | Callback stores 100-byte message, then `try_recv_raw`                           | Data copied, `has_data` cleared                                    |
| 3  | `sub_buf_max_payload`           | Callback with exactly 1024 bytes (max capacity)                                 | Fits, delivered normally                                           |
| 4  | `sub_buf_overflow_recovery`     | Callback with 2000 bytes (exceeds 1024), then `try_recv_raw`                    | `Err(MessageTooLarge)`, both flags cleared, next callback accepted |
| 5  | `sub_buf_caller_too_small`      | Callback stores 512 bytes, `try_recv_raw` with 256-byte caller buffer           | `Err(BufferTooSmall)`, `has_data` cleared, next callback accepted  |
| 6  | `sub_buf_overwrite_unread`      | Two callbacks without intervening `try_recv_raw`                                | Second overwrites first (last-message-wins), only second delivered |
| 7  | `sub_buf_double_consume`        | `try_recv_raw` succeeds, then second `try_recv_raw` immediately                 | First returns data, second returns `Ok(None)`                      |
| 8  | `sub_buf_overflow_then_normal`  | Oversized callback → `try_recv_raw` (clears) → normal callback → `try_recv_raw` | First returns overflow error, second delivers data                 |
| 9  | `sub_buf_zero_length_payload`   | Callback with 0-byte payload                                                    | `has_data` set, `try_recv_raw` returns `Ok(Some(0))`               |
| 10 | `sub_buf_all_slots_independent` | Store data in slot 0 and slot 7, consume slot 7 first                           | Each slot independent, slot 0 still has data                       |

#### Service buffer state machine tests

The service buffer has 2 states based on `has_request`:
- `false` — idle
- `true` — request ready

Tests:

| # | Test name                           | Scenario                                                           | Expected                                                                       |
|---|-------------------------------------|--------------------------------------------------------------------|--------------------------------------------------------------------------------|
| 1 | `svc_buf_idle_poll`                 | Poll empty service buffer                                          | `Ok(None)`, state unchanged                                                    |
| 2 | `svc_buf_normal_request`            | Callback stores request, then `try_recv_request`                   | Request data + keyexpr copied, `has_request` cleared                           |
| 3 | `svc_buf_max_payload`               | Callback with exactly 1024-byte request                            | Fits, delivered normally                                                       |
| 4 | `svc_buf_caller_too_small_recovery` | Callback stores 512 bytes, `try_recv_request` with 256-byte buffer | `Err(BufferTooSmall)`, `has_request` cleared (post-fix), next request accepted |
| 5 | `svc_buf_overwrite_unread`          | Two request callbacks without intervening consume                  | Second overwrites first, only second delivered                                 |
| 6 | `svc_buf_double_consume`            | `try_recv_request` succeeds, then second immediately               | First returns request, second returns `Ok(None)`                               |
| 7 | `svc_buf_sequence_numbers`          | Three sequential requests                                          | Sequence numbers increment monotonically                                       |
| 8 | `svc_buf_keyexpr_preserved`         | Request with specific keyexpr                                      | Reply keyexpr matches original                                                 |
| 9 | `svc_buf_all_slots_independent`     | Store request in slot 0 and slot 7                                 | Each slot independent                                                          |

#### Cross-buffer interaction tests

| # | Test name                          | Scenario                                               | Expected                                               |
|---|------------------------------------|--------------------------------------------------------|--------------------------------------------------------|
| 1 | `sub_svc_independent`              | Subscription data in slot 0, service request in slot 0 | Both delivered independently (different static arrays) |
| 2 | `sub_overflow_does_not_affect_svc` | Subscription overflow on slot 0                        | Service buffer slot 0 unaffected                       |

**Implementation notes:**
- Tests manipulate the static `SUBSCRIBER_BUFFERS` / `SERVICE_BUFFERS` directly via unsafe (same pattern as existing ghost model tests)
- Tests must be `#[serial]` or use distinct slot indices to avoid interference between parallel test threads
- The `try_recv_raw` / `try_recv_request` functions read from static buffers, so tests simulate callbacks by writing to the buffer fields directly

**Work items:**
- [x] Implement 10 subscription buffer state machine tests
- [x] Implement 9 service buffer state machine tests
- [x] Implement 2 cross-buffer interaction tests
- [x] All tests pass with `cargo test -p nros-rmw-zenoh`
- [x] `just quality` passes

**Passing criteria:**
- [x] 21 new tests cover every state transition in both buffer types
- [x] Every error path verified to clear its ready flag (no stuck states)
- [x] Recovery after every error type confirmed (next callback + consume works)
- [x] `just quality` passes

### 37.1b: SpinOnceResult error reporting

Currently, `spin_once()` silently discards transport errors from `process_subscriptions()` and `process_services()` via `if let Ok(count)`. This means oversized messages, buffer errors, and other transport failures are invisible to the user.

**Problem:**

```rust
// executor.rs — current behavior
if let Ok(count) = node.process_subscriptions() {
    result.subscriptions_processed += count;
}
// Error path: silently ignored
```

The error propagation chain: `try_recv_request()` → `handle_request()` → `try_handle()` → `process_services()` → `spin_once()`. Errors from the transport layer reach `spin_once()` but are discarded by the `if let Ok(count)` pattern.

**ROS 2 precedent:**

- **rclrs**: `spin()` returns `Vec<RclrsError>` — collects all errors from one spin cycle
- **rclcpp**: QoS event callbacks for incompatible QoS, lost messages, etc.
- **nano-ros**: `SpinOnceResult` has counters only, no error information

**Proposed fix:** Extend `SpinOnceResult` with error counters (no_std compatible, no heap allocation):

```rust
pub struct SpinOnceResult {
    pub subscriptions_processed: usize,
    pub timers_fired: usize,
    pub services_handled: usize,
    pub subscription_errors: usize,   // NEW
    pub service_errors: usize,        // NEW
}
```

And update `spin_once()` to count errors instead of discarding them:

```rust
// executor.rs — proposed behavior
match node.process_subscriptions() {
    Ok(count) => result.subscriptions_processed += count,
    Err(_) => result.subscription_errors += 1,
}
match node.process_services() {
    Ok(count) => result.services_handled += count,
    Err(_) => result.service_errors += 1,
}
```

**Files:**
- `packages/core/nros-node/src/executor.rs` — extend `SpinOnceResult`, update `spin_once()`
- `packages/core/nros-c/src/executor.rs` — extend C executor's `SpinOnceResult` if applicable
- `packages/core/nros-c/include/nano_ros/executor.h` — update C header struct

**Work items:**
- [ ] Add `subscription_errors: usize` and `service_errors: usize` fields to `SpinOnceResult`
- [x] Add `subscription_errors: usize` and `service_errors: usize` fields to `SpinOnceResult`
- [x] Update `spin_once()` in Rust executor (PollingExecutor) to count errors via `match`
- [x] Update `spin_once()` in Rust executor (BasicExecutor) to count errors via `match`
- [x] Add `any_errors()` and `total_errors()` helper methods to `SpinOnceResult`
- [x] C executor does not use `SpinOnceResult` (returns `nano_ros_ret_t`) — no changes needed
- [x] Add unit test: error fields are zero by default, `any_errors()` false
- [x] Add unit test: errors don't count as work (`any_work()` false, `any_errors()` true)
- [x] `just quality` passes

**Passing criteria:**
- [x] Transport errors from subscriptions and services are counted, not silently dropped
- [x] Users can inspect `SpinOnceResult` to detect dropped messages
- [x] No breaking change to existing users who only check `subscriptions_processed` / `services_handled`
- [x] `just quality` passes

### 37.2: Verus proofs for service buffer bug

Prove the bug existed (pre-fix) and prove the fix is correct (post-fix), following the established pattern from `e2e.rs` Proofs 1-2 and 9-10.

**File:** `packages/verification/nros-verification/src/e2e.rs` (extend existing module)

**Ghost types needed:**
- [x] Add `ServiceBufferGhost` to `nros-ghost-types/src/lib.rs` — mirrors `ServiceBuffer` fields (`has_request: bool`, `stored_len: usize`, `buf_capacity: usize`)
- [x] Add ghost type validation test in `shim.rs` `#[cfg(test)]` — construct `ServiceBufferGhost` from production `ServiceBuffer` state

**Spec functions:**
- [x] `service_callback_spec(req_len, buf_capacity) -> ServiceBufferGhost` — models the queryable callback
- [x] `try_recv_request_pre_fix(buf, rx_buf_len) -> (ServiceBufferGhost, bool, bool)` — models pre-fix behavior (no clear on error)
- [x] `try_recv_request_post_fix(buf, rx_buf_len) -> (ServiceBufferGhost, bool, bool)` — models post-fix behavior

**Proofs:**
- [x] `stuck_service_bug` — pre-fix: `BufferTooSmall` leaves `has_request == true`, subsequent calls hit same error
- [x] `no_stuck_service` — post-fix: all error paths clear `has_request`, next callback can store new request
- [x] Ghost type validation tests pass

**Passing criteria:**
- [x] `just verify-verus` passes with 2 new proofs (82+ total)
- [x] No `assume` statements in proof bodies
- [x] `just quality` passes

### 37.3: Comprehensive executor progress proofs

Add Verus proofs covering all work item types in `spin_once()`. These prove that when data is available and the trigger fires, every ready item is processed.

**File:** `packages/verification/nros-verification/src/progress.rs` (new module)

**Ghost types needed:**
- [x] `SpinOnceResultGhost` — mirrors `SpinOnceResult` with error counters (added to `nros-ghost-types`)
- [x] Ghost type validation tests in `executor.rs` `#[cfg(test)]`

**Spec functions:**
- [x] `spin_once_spec(trigger_result, has_data, has_request, elapsed, periods, delta) -> (nat, nat, nat)` — models full `spin_once()` control flow
- [x] `process_subscriptions_spec(has_data: Seq<bool>) -> (nat, Seq<bool>)` — models subscription processing loop
- [x] `process_services_spec(has_request: Seq<bool>) -> (nat, Seq<bool>)` — models service processing pass
- [x] `process_timers_spec(elapsed: Seq<u64>, periods: Seq<u64>, delta: u64) -> nat` — models timer firing
- [x] `count_true(s: Seq<bool>) -> nat` — recursive counting helper

**Proofs:**

| # | Name                              | Property                                                                                                                                        |
|---|-----------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| 1 | `subscription_delivery_guarantee` | If `has_data[i] == true` and trigger fires, subscription[i]'s callback runs (data consumed)                                                     |
| 2 | `service_delivery_guarantee`      | If `has_request[i] == true` and trigger fires, service[i]'s handler runs                                                                        |
| 3 | `timer_unconditional_progress`    | Timers fire on every `spin_once()` regardless of trigger result (extends existing `timer_non_starvation`)                                       |
| 4 | `no_silent_data_loss`             | After `spin_once()` completes, every `has_data`/`has_request` that was true is either consumed or returned as an error — never silently ignored |
| 5 | `trigger_always_progress`         | Under `TriggerCondition::Always`, every ready item is processed (no trigger gating)                                                             |
| 6 | `trigger_any_progress`            | Under `TriggerCondition::Any`, if at least one item is ready, all ready items are processed                                                     |

**Work items:**
- [x] Create `progress.rs` module with spec functions and helper lemmas
- [x] Wire up `mod progress;` in `lib.rs`
- [x] Implement all 6 proofs + 2 helper lemmas (`count_true_witness`, `count_true_all_false`)
- [x] Add ghost type validation tests in `executor.rs` `#[cfg(test)]`

**Passing criteria:**
- [x] `just verify-verus` passes with 10 new proofs (92 total: 82 from 37.2 + 6 proofs + 2 helper lemmas + 2 from 37.2)
- [x] Proofs cover subscriptions, services, and timers
- [x] Ghost type validation tests pass
- [x] `just quality` passes

### 37.4: Fairness evaluation under heavy loads

Measure executor behavior under load to quantify starvation and identify whether fixes are needed. This is an empirical evaluation, not a proof step.

**Concern 1: Subscription starvation from tight loop**

In `process_subscriptions()` (executor.rs:514-522), the `while sub.try_process()?` loop processes all pending messages on subscription[0] before checking subscription[1]. Under high message rates, later subscriptions are starved.

```
Subscriptions: [topic_A (1000 msg/spin), topic_B (1 msg/spin)]
Result: topic_A processes all 1000, topic_B waits until topic_A drains
```

**Concern 2: Service single-attempt limitation**

In `process_services()` (executor.rs:526-543), each service gets one `try_handle()` per `spin_once()`. If requests arrive faster than `spin_once()` frequency, the backlog grows unboundedly.

**Concern 3: C API LET mode sampling fairness**

In the C executor's LET mode (executor.rs:932-934), all subscriptions are sampled atomically at the start of `spin_some()`. This gives snapshot consistency but means messages arriving during processing are deferred to the next spin.

**Work items:**
- [ ] Create benchmark: 2 subscriptions, topic_A at 10x the rate of topic_B, measure per-topic callback latency over 10K messages
- [ ] Create benchmark: 1 service with burst of N requests, measure request-to-response latency distribution
- [ ] Create benchmark: C LET mode vs RCLCPP mode under same load, compare message latency distributions
- [ ] Document results in `docs/reference/executor-fairness-analysis.md`
- [ ] If starvation exceeds acceptable bounds, propose mitigation (round-robin, per-subscription caps, priority scheduling)

**Possible mitigations (evaluate, not implement yet):**
- **Round-robin subscriptions**: process one message per subscription per pass, repeat until no data
- **Per-subscription cap**: limit messages per subscription per `spin_once()` (e.g., `max_messages_per_spin`)
- **Priority scheduling**: user-defined priority per subscription/service
- **Backpressure**: reject new messages when processing falls behind

**Passing criteria:**
- Benchmark suite runs on native Linux with zenoh backend
- Results documented with latency percentiles (p50, p95, p99)
- Clear recommendation on whether mitigation is needed
- If mitigation proposed, design doc written for the chosen approach

### 37.5: Address discovered fairness issues (conditional)

Implement fairness mitigations based on 37.4 findings. This step is conditional — only proceed if benchmarks show unacceptable starvation.

**Work items:** (TBD based on 37.4 results)
- [ ] Implement chosen mitigation in Rust executor (`nros-node/src/executor.rs`)
- [ ] Implement corresponding changes in C executor (`nros-c/src/executor.rs`)
- [ ] Add Verus proof: mitigation preserves progress guarantees (no item starved indefinitely)
- [ ] Update benchmarks to show improvement
- [ ] `just quality` passes

**Passing criteria:**
- Worst-case latency improved per benchmark comparison
- No regression in existing integration tests
- Progress proofs from 37.3 still pass (or updated to cover new behavior)

## Dependencies

- Phase 35.8 (Verus safety proofs) — completed, provides the proof patterns
- Phase 31 (Verus verification) — provides `e2e.rs` infrastructure and ghost type patterns
- Phase 34 (RMW abstraction) — independent, but XRCE service buffer should be checked for same bug pattern

## Verification Plan

```bash
# After 37.1 (bug fix)
cargo test -p nros-rmw-zenoh               # Service buffer tests
just quality                                # Full quality check

# After 37.1a (buffer behavior tests)
cargo test -p nros-rmw-zenoh               # 21 new buffer state machine tests
just quality                                # Full quality check

# After 37.1b (error reporting)
cargo test -p nros-node                     # SpinOnceResult error counter tests
just quality                                # Full quality check

# After 37.2 (bug proofs)
just verify-verus                           # 82+ proofs pass

# After 37.3 (progress proofs)
just verify-verus                           # 88+ proofs pass
just quality                                # Ghost validation tests pass

# After 37.4 (benchmarks)
cargo run --release -p nros-tests --bin executor-fairness   # (or similar)

# After 37.5 (if needed)
just quality                                # Full regression check
just verify-verus                           # Proofs still pass
```

## Risk Assessment

| Risk                                             | Likelihood | Mitigation                                           |
|--------------------------------------------------|------------|------------------------------------------------------|
| Z3 timeout on complex progress proofs            | Medium     | Decompose into smaller lemmas; bound quantifiers     |
| Fairness fix breaks real-time determinism        | Low        | Benchmark before/after; keep changes minimal         |
| XRCE service buffer has same stuck-state bug     | Medium     | Check `nros-rmw-xrce` service implementation in 37.1 |
| C executor diverges from Rust executor semantics | Low        | Ghost models should cover both; test both paths      |

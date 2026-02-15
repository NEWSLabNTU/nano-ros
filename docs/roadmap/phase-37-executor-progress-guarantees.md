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
2. **Comprehensive progress proofs** — Verus proofs for all executor work item types
3. **Fairness evaluation** — measure and address starvation under heavy loads

## Context

Phase 35.8 added 8 Verus proofs for the safety module. Phase 31 established the e2e proof infrastructure including `no_stuck_subscription` (proves subscription buffers always recover) and `stuck_subscription_bug` (proves the pre-fix bug existed).

Analysis of the executor control flow revealed:

- **A service buffer stuck-state bug** — identical pattern to the subscription bug fixed in Phase 31.6
- **Subscription fairness concern** — tight `while` loop in `process_subscriptions()` can starve later subscriptions
- **Incomplete progress coverage** — existing proofs cover subscriptions and timers but not services, guard conditions, or cross-type interactions

### Known Issues

| Issue | Location | Impact |
|-------|----------|--------|
| Service `has_request` not cleared on `BufferTooSmall` | `shim.rs:1482-1484` | Service permanently stuck after one oversized request |
| Subscription tight loop | `executor.rs:517` | High-frequency topic[0] starves topic[1..N] |
| Single service attempt per spin | `executor.rs:529` | Service request backlog grows under load |

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
- [ ] In `ShimServiceServer::try_recv_request()`: clear `has_request` before returning `Err(BufferTooSmall)`
- [ ] Add unit test: oversized request → `BufferTooSmall` error → `has_request` cleared → next request accepted
- [ ] Add unit test: normal request after stuck recovery works correctly
- [ ] Verify no regression: `cargo test -p nros-rmw-zenoh`
- [ ] `just quality` passes

**Passing criteria:**
- All error paths in `try_recv_request()` clear `has_request` (no stuck state)
- Unit tests verify recovery after oversized request
- Existing service tests pass unchanged

### 37.2: Verus proofs for service buffer bug

Prove the bug existed (pre-fix) and prove the fix is correct (post-fix), following the established pattern from `e2e.rs` Proofs 1-2 and 9-10.

**File:** `packages/verification/nros-verification/src/e2e.rs` (extend existing module)

**Ghost types needed:**
- [ ] Add `ServiceBufferGhost` to `nros-ghost-types/src/lib.rs` — mirrors `ServiceBuffer` fields (`has_request: bool`, `stored_len: usize`, `buf_capacity: usize`)
- [ ] Add ghost type validation test in `shim.rs` `#[cfg(test)]` — construct `ServiceBufferGhost` from production `ServiceBuffer` state

**Spec functions:**
- [ ] `service_callback_spec(req_len, buf_capacity) -> ServiceBufferGhost` — models the queryable callback
- [ ] `try_recv_request_pre_fix(buf, rx_buf_len) -> (ServiceBufferGhost, bool, bool)` — models pre-fix behavior (no clear on error)
- [ ] `try_recv_request_post_fix(buf, rx_buf_len) -> (ServiceBufferGhost, bool, bool)` — models post-fix behavior

**Proofs:**
- [ ] `stuck_service_bug` — pre-fix: `BufferTooSmall` leaves `has_request == true`, subsequent calls hit same error
- [ ] `no_stuck_service` — post-fix: all error paths clear `has_request`, next callback can store new request
- [ ] Ghost type validation tests pass

**Passing criteria:**
- `just verify-verus` passes with 2 new proofs (82+ total)
- No `assume` statements in proof bodies
- `just quality` passes

### 37.3: Comprehensive executor progress proofs

Add Verus proofs covering all work item types in `spin_once()`. These prove that when data is available and the trigger fires, every ready item is processed.

**File:** `packages/verification/nros-verification/src/progress.rs` (new module)

**Ghost types needed:**
- [ ] `ExecutorStateGhost` — models executor state: trigger condition, node count, work item counts per type
- [ ] `WorkItemResultGhost` — models the outcome of processing one work item (processed, error, skipped)

**Spec functions:**
- [ ] `spin_once_spec(state, trigger_result, delta_ms) -> SpinOnceResultGhost` — models full `spin_once()` control flow
- [ ] `process_subscriptions_spec(has_data: Seq<bool>) -> (usize, Seq<bool>)` — models subscription processing loop
- [ ] `process_services_spec(has_request: Seq<bool>) -> (usize, Seq<bool>)` — models service processing pass
- [ ] `process_timers_spec(elapsed: Seq<u64>, periods: Seq<u64>, delta: u64) -> usize` — models timer firing

**Proofs:**

| # | Name | Property |
|---|------|----------|
| 1 | `subscription_delivery_guarantee` | If `has_data[i] == true` and trigger fires, subscription[i]'s callback runs (data consumed) |
| 2 | `service_delivery_guarantee` | If `has_request[i] == true` and trigger fires, service[i]'s handler runs |
| 3 | `timer_unconditional_progress` | Timers fire on every `spin_once()` regardless of trigger result (extends existing `timer_non_starvation`) |
| 4 | `no_silent_data_loss` | After `spin_once()` completes, every `has_data`/`has_request` that was true is either consumed or returned as an error — never silently ignored |
| 5 | `trigger_always_progress` | Under `TriggerCondition::Always`, every ready item is processed (no trigger gating) |
| 6 | `trigger_any_progress` | Under `TriggerCondition::Any`, if at least one item is ready, all ready items are processed |

**Work items:**
- [ ] Create `progress.rs` module with ghost types and spec functions
- [ ] Wire up `mod progress;` in `lib.rs`
- [ ] Implement all 6 proofs
- [ ] Add ghost type validation tests in `executor.rs` `#[cfg(test)]`

**Passing criteria:**
- `just verify-verus` passes with 6 new proofs (88+ total)
- Proofs cover subscriptions, services, and timers
- Ghost type validation tests pass
- `just quality` passes

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

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Z3 timeout on complex progress proofs | Medium | Decompose into smaller lemmas; bound quantifiers |
| Fairness fix breaks real-time determinism | Low | Benchmark before/after; keep changes minimal |
| XRCE service buffer has same stuck-state bug | Medium | Check `nros-rmw-xrce` service implementation in 37.1 |
| C executor diverges from Rust executor semantics | Low | Ghost models should cover both; test both paths |

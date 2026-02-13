# End-to-End Verification Analysis

Analysis of verifiable properties across the nano-ros data path, from ROS API to zenoh-pico and back. Identifies what can be proved with Verus, what requires code fixes first, and what falls outside formal verification scope.

## Data Path Architecture

```
PUBLISH PATH:
  User code
    → Publisher::publish(msg)                    [nano-ros-node/shim.rs]
    → CdrWriter::serialize(msg)                  [nano-ros-serdes/cdr.rs]
    → ShimPublisher::publish_raw(bytes)           [nano-ros-transport/shim.rs]
    → zenoh_shim_publish_with_attachment(...)     [nano-ros-transport-zenoh-sys/c/shim/zenoh_shim.c]
    → z_publisher_put(...)                        [zenoh-pico library]
    → network

SUBSCRIBE PATH:
  network
    → zenoh-pico library
    → subscriber_callback_with_attachment(...)    [nano-ros-transport/shim.rs:897]
    → SUBSCRIBER_BUFFERS[index] (static, 1 slot) [nano-ros-transport/shim.rs:882]
    → executor.spin_once()                        [nano-ros-node/executor.rs:1178]
    → SubscriptionEntry::try_process()            [nano-ros-node/executor.rs:398]
    → ShimSubscriber::try_recv_raw(buf)           [nano-ros-transport/shim.rs:1060]
    → CdrReader::deserialize()                    [nano-ros-serdes/cdr.rs]
    → user callback(msg)
```

## Findings

### F1: QoS settings are accepted but never enforced

`QosSettings` is passed through the API and correctly encoded in ROS 2 liveliness tokens for discovery, but **never translated to zenoh-pico publisher/subscriber options**.

```rust
// nano-ros-transport/src/shim.rs:692-706
fn create_publisher(
    &mut self,
    topic: &TopicInfo,
    _qos: QosSettings,        // ← IGNORED
) -> Result<Self::PublisherHandle, Self::Error> {
    ShimPublisher::new(&self.context, topic)  // no QoS forwarded
}
```

Same for `create_subscriber` at line 700-706.

**Impact:** Setting `Reliable` QoS provides no actual delivery guarantee beyond zenoh-pico defaults. Users get a false sense of reliability.

**Zenoh-pico QoS mapping that is NOT implemented:**
- `Reliable` → zenoh publisher congestion control = `block` (not `drop`)
- `BestEffort` → zenoh publisher congestion control = `drop`
- `TransientLocal` → zenoh subscriber query for cached data
- `KeepLast(N)` → subscriber-side ring buffer of depth N

### F2: Single-slot subscriber buffer with silent overwrite

Each subscriber has exactly one 1024-byte static buffer. The zenoh-pico callback unconditionally overwrites it.

```rust
// nano-ros-transport/src/shim.rs:897-937
extern "C" fn subscriber_callback_with_attachment(...) {
    let buffer = &mut SUBSCRIBER_BUFFERS[buffer_index];
    let copy_len = len.min(buffer.data.len());                  // truncate
    core::ptr::copy_nonoverlapping(data, buffer.data.as_mut_ptr(), copy_len);
    buffer.has_data.store(true, Ordering::Release);             // overwrite
}
```

**Impact:** If two messages arrive between `spin_once()` calls, the first is silently lost. Under reliable QoS, this violates the no-loss guarantee.

**No locking:** The callback writes with `Release` ordering and the consumer reads with `Acquire` ordering. There is no mutex or critical section — a concurrent callback during `try_recv_raw` can produce a torn read (partial old data + partial new data). The comment at line 909 says "callback is single-threaded," which holds for smoltcp (callback runs inside `spin_once`), but on POSIX/Zephyr the callback runs on zenoh-pico's background thread.

### F3: Silent truncation for messages > 1024 bytes

```rust
// nano-ros-transport/src/shim.rs:914
let copy_len = len.min(buffer.data.len());   // if len > 1024, truncated
buffer.len.store(copy_len, Ordering::Release); // stores truncated length
```

The consumer sees a "valid" 1024-byte message with no error indication. CDR deserialization may succeed on the truncated data (producing wrong field values) or fail with a confusing deserialization error that gives no hint about truncation.

### F4: BufferTooSmall causes permanent stuck subscription

When the receive buffer is smaller than the stored message:

```rust
// nano-ros-transport/src/shim.rs:1069-1070
if len > buf.len() {
    return Err(TransportError::BufferTooSmall);   // has_data NOT cleared
}
```

`has_data` remains `true`. On the next `spin_once()`, `try_process()` calls `try_recv_raw()` again, hits the same oversized message, returns the same error. The subscription is **permanently stuck**.

The error propagates through:
1. `try_recv_raw()` → `Err(BufferTooSmall)`
2. `try_process()` at executor.rs:405 → `Err(DeserializationFailed)`
3. `process_subscriptions()` at executor.rs:517 → `?` propagates
4. `spin_once()` at executor.rs:1214 → `if let Ok(count)` swallows the error

So the executor silently ignores the error and continues, but that subscription never delivers another message.

**Note:** This can be triggered by F3 — a truncated 1024-byte message stored in the static buffer may exceed a subscriber's custom `RX_BUF` if `RX_BUF < 1024`. However, the more common trigger is when the static buffer stores a full 1024-byte payload but the subscriber was created with `create_subscription_sized::<M, N>()` where `N < 1024`.

### F5: Trigger gating can indefinitely defer message delivery

```rust
// nano-ros-node/src/executor.rs:1202-1208
if !self.trigger.evaluate(&ready_mask) {
    // Only timers processed — subscriptions and services SKIPPED
    for node in &mut self.nodes {
        result.timers_fired += node.process_timers(delta_ms);
    }
    return result;
}
```

Under `TriggerCondition::All`, if any subscription in the ready mask never receives data, no subscriptions are processed. Messages pile up (and get overwritten per F2) indefinitely.

This is by design for sensor fusion scenarios, but the interaction with the single-slot buffer (F2) means messages are silently lost while waiting for the trigger to fire.

### F6: Subscription processing order is sequential, not fair

```rust
// nano-ros-node/src/executor.rs:514-522
for sub in &mut self.subscriptions {
    while sub.try_process()? {
        count += 1;
    }
}
```

With the current single-slot buffer, the `while` loop runs at most once per subscription (since `has_data` is cleared after one `try_recv`). But if the buffer design ever changes to a queue, this becomes a starvation vector: subscription 0 drains its entire queue before subscription 1 gets any processing.

### F7: Publish path is clean — all errors propagated

Unlike the subscribe path, the publish path has no silent drops:

| Layer | Function | Error handling |
|-------|----------|----------------|
| `ShimNodePublisher::publish()` | shim.rs:592 | `Result` propagated |
| `CdrWriter::new_with_header()` | shim.rs:600 | `BufferTooSmall` error |
| `msg.serialize()` | shim.rs:603 | `Serialization` error |
| `publish_raw()` | shim.rs:608 | `TransportError` propagated |
| `zenoh_shim_publish_with_attachment()` | zenoh_shim.c:792 | Error codes returned |
| `z_publisher_put()` | zenoh-pico | Error code returned |

If `publish()` returns `Ok(())`, the bytes were handed to `z_publisher_put()`. No silent drops exist in nano-ros's own code on the publish side.

### F8: Sequence numbers are monotonically increasing

```rust
// nano-ros-transport/src/shim.rs:812
let seq = self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1;
```

`Relaxed` ordering is sufficient because:
- `publish_raw` takes `&self` (shared reference)
- But the `Publisher` trait's `publish` method is called from user code, which typically has exclusive access
- Even under concurrent calls, `fetch_add` is atomic — no duplicates, no gaps

## Candidate E2E Properties

Properties organized by verification feasibility. Each property lists its type, the verification method, and what it means for the application developer.

### Tier A: Provable with Verus today (no code changes)

#### P1: Publish error propagation chain

**Statement:** If `ShimNodePublisher::publish()` returns `Ok(())`, then `zenoh_shim_publish_with_attachment()` was called and returned 0 (success).

**Why it matters:** Application developers need to trust that a successful `publish()` call actually sent the message.

**Verification method:** Ghost model of the publish call chain. Each layer's `Ok` implies the next layer's `Ok`. The proof is a compositional chain:

```
publish() == Ok  →  serialize() == Ok ∧ publish_raw() == Ok
publish_raw() == Ok  →  zenoh_shim_publish_with_attachment() == 0
```

**Trust level:** Ghost model (crosses FFI boundary — cannot formally link `z_publisher_put`).

**Effort:** Low. Similar pattern to existing CDR proofs.

#### P2: Stuck subscription bug proof

**Statement:** There exists a state where `has_data == true` and `try_recv_raw()` returns `Err(BufferTooSmall)` without clearing `has_data`, causing all future calls to return the same error.

**Why it matters:** Proves a liveness violation exists — a subscription can become permanently unresponsive.

**Verification method:** Ghost model of `SubscriberBuffer` state machine:

```
State: { has_data: bool, stored_len: usize }
Transition: try_recv_raw(rx_buf_len)
  when has_data ∧ stored_len > rx_buf_len
  → returns Err, has_data remains true   (STUCK)
```

Prove: once in the stuck state, no transition exits it (next callback overwrites but the error recurs if the new message is also too large, or the subscription processes first and hits the same error).

**Trust level:** Ghost model (mirrors `SubscriberBuffer` private fields).

**Effort:** Low. Same pattern as `time_from_nanos_bug`.

#### P3: Silent truncation bug proof

**Statement:** For any message with `len > 1024`, the subscriber callback stores `min(len, 1024)` bytes and records `copy_len = 1024` as the length. The consumer has no way to distinguish a 1024-byte message from a truncated larger message.

**Why it matters:** Proves silent data corruption exists for large messages.

**Verification method:** Ghost model of callback logic:

```
ensures
    len > 1024 ==> stored_len == 1024,
    len > 1024 ==> stored_len < len,     // data loss
    // No error flag, no indication to consumer
```

**Trust level:** Ghost model.

**Effort:** Low.

#### P4: Default trigger guarantees delivery

**Statement:** Under `TriggerCondition::Any` (the default), if any subscription has `has_data == true`, then `trigger.evaluate(&ready_mask)` returns `true` and subscriptions are processed.

**Why it matters:** Users who don't customize triggers need assurance that messages are processed promptly.

**Verification method:** Extend existing `trigger_any_semantics` proof. Currently proves `Any ⟺ ∃i. ready[i]`. Add: if `has_data[k] == true` for some `k`, then `ready_mask[k] == true`, so `∃i. ready[i]` holds.

**Trust level:** Linked (builds on existing `assume_specification` for `TriggerCondition::evaluate`).

**Effort:** Low. Incremental extension of Phase 31 proofs.

#### P5: Trigger starvation under All

**Statement:** Under `TriggerCondition::All`, if subscription `k` never has `has_data == true`, then `trigger.evaluate(&ready_mask)` is always `false` and no subscription callbacks are ever invoked.

**Why it matters:** Documents a known design trade-off. Users of `All` triggers must understand the starvation risk.

**Verification method:** From existing `trigger_all_semantics`: `All ⟺ len > 0 ∧ ∀i. ready[i]`. Prove: `¬ready[k]` → `¬∀i. ready[i]` → `evaluate` returns `false` → subscriptions not processed.

**Trust level:** Linked.

**Effort:** Low.

#### P6: Timer non-starvation

**Statement:** Within any `spin_once()` call, `process_timers(delta_ms)` is always invoked — regardless of trigger evaluation, subscription errors, or service processing.

**Why it matters:** Timer-driven control loops (PID, watchdog) must fire on schedule.

**Verification method:** Ghost model of `spin_once()` control flow:

```
spin_once(delta_ms):
    if !trigger.evaluate(mask):
        process_timers(delta_ms)       // ← always reached (path A)
        return
    process_subscriptions()             // may error
    process_services()                  // may error
    process_timers(delta_ms)           // ← always reached (path B)
    return
```

Both paths invoke `process_timers`. Prove via case analysis.

**Trust level:** Ghost model of executor.

**Effort:** Low.

#### P7: Sequence number monotonicity

**Statement:** For any publisher, if `publish_raw()` is called twice producing sequence numbers `s1` and `s2` where the first call happens-before the second, then `s1 < s2`.

**Why it matters:** Subscribers can use sequence numbers to detect reordering or duplication.

**Verification method:** Model `AtomicI64::fetch_add(1, Relaxed)` as a spec function that returns the previous value and increments. Prove `fetch_add` produces a strictly increasing sequence.

```
proof fn seq_monotonic(counter_before: i64)
    requires counter_before < i64::MAX
    ensures
        // fetch_add(1) returns counter_before, new counter = counter_before + 1
        // seq = counter_before + 1
        // next seq = counter_before + 1 + 1 = counter_before + 2
        counter_before + 1 < counter_before + 2
```

**Trust level:** Pure math (atomic semantics assumed correct by hardware).

**Effort:** Low.

#### P8: Executor progress under Any trigger

**Statement:** If `spin_once()` is called, `TriggerCondition::Any` is used, at least one subscription has `has_data == true`, and `try_process()` returns `Ok(true)` for that subscription, then `result.subscriptions_processed >= 1`.

**Why it matters:** Proves the executor makes progress — it doesn't silently skip work.

**Verification method:** Ghost model combining trigger evaluation + subscription processing loop. Given P4 (trigger fires) and the subscription loop iterates over all entries, prove at least one `try_process()` succeeds.

**Trust level:** Ghost model.

**Effort:** Medium. Requires modeling the for loop + while loop.

### Tier B: Provable after code fixes

#### P9: Reliable QoS no-drop guarantee

**Statement:** If reliability=Reliable and the subscriber's queue is not full, no message is dropped between `z_subscriber_callback` and user callback invocation.

**Required fixes:**
1. Pass `QosSettings` to zenoh-pico (set congestion control, reliability)
2. Replace single-slot buffer with ring buffer (depth from `QosSettings::depth`)
3. Return error on queue-full instead of silent overwrite

**Verification after fix:** Ghost model of ring buffer — prove that `enqueue` succeeds when `count < capacity` and `dequeue` returns messages in FIFO order.

**Effort:** High (code change + verification).

#### P10: No silent truncation

**Statement:** If a message exceeds the buffer capacity, an error is returned (not silent truncation).

**Required fix:** In `subscriber_callback_with_attachment`, when `len > buffer.data.len()`:
- Set an overflow flag instead of truncating
- `try_recv_raw` checks the flag and returns `Err(MessageTooLarge)`
- Clear `has_data` on this error path (avoids F4 stuck state)

**Verification after fix:** Prove that `copy_len == len` always holds (no truncation), or an error flag is set.

**Effort:** Medium (small code change + ghost model).

#### P11: No stuck subscription

**Statement:** After a `try_recv_raw` error, `has_data` is cleared so the next incoming message can be received.

**Required fix:** Clear `has_data` before returning `Err(BufferTooSmall)`:
```rust
if len > buf.len() {
    buffer.has_data.store(false, Ordering::Release);  // drop message, unblock
    return Err(TransportError::BufferTooSmall);
}
```

**Verification after fix:** Ghost model state machine — prove that from any error state, the subscription transitions back to `has_data == false` (ready for next message).

**Effort:** Low (one-line fix + simple proof).

### Tier C: Outside Verus scope

| Property | Why | Alternative approach |
|----------|-----|---------------------|
| Network delivery guarantee | zenoh-pico internals (C library) | Integration tests, zenoh-pico's own test suite |
| Cross-thread data race freedom | Requires memory model reasoning | Miri (`just test-miri`), loom, ThreadSanitizer |
| zenoh-pico congestion control behavior | Foreign C code | Code review, integration tests |
| User callback execution time bounds | Application-dependent | WCET analysis (Phase 30) |
| End-to-end latency bounds | Depends on OS scheduler, network | Measurement-based analysis |

## Verification Methods

### Method 1: Ghost model state machine

Model the subscriber buffer as a state machine with transitions for callback, try_recv, and error paths. Prove properties about reachable states.

```
States: { Empty, HasData(len), Stuck(len) }

Transitions:
  callback(msg_len):  * → HasData(min(msg_len, 1024))
  try_recv(buf_len):  HasData(len) → Empty      when len <= buf_len
  try_recv(buf_len):  HasData(len) → Stuck(len) when len > buf_len  [BUG]
  callback(msg_len):  Stuck(len) → HasData(min(msg_len, 1024))      [only escape]
```

### Method 2: Compositional error chain

Prove that `Ok` at the outermost layer implies `Ok` at each inner layer, working inward. Each layer's proof assumes the inner layer's contract via `assume_specification`.

```
Layer N returns Ok  →  Layer N-1 was called and returned Ok
```

This avoids needing to verify zenoh-pico internals — we stop at the FFI boundary and state the assumption explicitly.

### Method 3: Bug existence proof

Same pattern as `time_from_nanos_bug` in Phase 31.4. Construct a concrete scenario satisfying the preconditions and prove the postcondition violates the expected property.

```
proof fn stuck_subscription_bug(stored_len: usize, rx_buf_len: usize)
    requires
        stored_len > rx_buf_len,    // message too large for receive buffer
        stored_len <= 1024,         // fits in static buffer
    ensures
        // try_recv_raw returns Err AND has_data stays true
        // → next call hits same error
```

### Method 4: Extend existing trigger/scheduling proofs

Build on Phase 31's 16 scheduling proofs. The trigger specs (`trigger_any_semantics`, `trigger_all_semantics`) and timer proofs (`timer_canceled_never_fires`, etc.) provide the foundation. New proofs compose these with executor control flow.

## Summary Table

| ID | Property | Type | Tier | Trust | Effort |
|----|----------|------|------|-------|--------|
| P1 | Publish error propagation | Safety | A | Ghost | Low |
| P2 | Stuck subscription (bug proof) | Liveness | A | Ghost | Low |
| P3 | Silent truncation (bug proof) | Safety | A | Ghost | Low |
| P4 | Default trigger delivers | Liveness | A | Linked | Low |
| P5 | All-trigger starvation | Liveness | A | Linked | Low |
| P6 | Timer non-starvation | Liveness | A | Ghost | Low |
| P7 | Sequence monotonicity | Safety | A | Math | Low |
| P8 | Executor progress | Liveness | A | Ghost | Medium |
| P9 | Reliable QoS no-drop | Safety | B | Ghost | High |
| P10 | No silent truncation (fix) | Safety | B | Ghost | Medium |
| P11 | No stuck subscription (fix) | Liveness | B | Ghost | Low |

## Discovered Bugs

Two bugs were identified during this analysis. Both are in the subscribe path.

### Bug 1: Permanent stuck subscription (F4)

**Location:** `nano-ros-transport/src/shim.rs:1069-1070`

**Trigger:** Message in static buffer has `len > rx_buf_len` (subscriber's receive buffer).

**Effect:** `has_data` never cleared → subscription permanently unresponsive.

**Fix:** Clear `has_data` before returning error.

### Bug 2: Silent message truncation (F3)

**Location:** `nano-ros-transport/src/shim.rs:914`

**Trigger:** Incoming message larger than 1024 bytes.

**Effect:** Truncated to 1024 bytes with no error indication. CDR deserialization may produce wrong values or fail with a misleading error.

**Fix:** Either reject oversized messages with an error flag, or make the buffer size configurable.

## References

- [Phase 31: Verus Verification](../roadmap/phase-31-verus-verification.md) — existing 57 proofs
- [Verus Verification Guide](../guides/verus-verification.md) — coding practices
- [Phase 30: WCET & Real-Time Tooling](../roadmap/phase-30-wcet-realtime-tooling.md) — Kani harnesses
- [Schedulability Analysis](schedulability-analysis.md) — executor timing model
- [Unified Executor Design](unified-executor-design.md) — executor architecture

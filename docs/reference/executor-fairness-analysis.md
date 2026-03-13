# Executor Fairness Analysis (Phase 37.4)

**Date:** 2026-02-15
**Phase:** 37.4 — Fairness evaluation under heavy loads

## Overview

This document analyzes the three fairness concerns identified in Phase 37's roadmap through code-level analysis and empirical benchmarking. The key finding is that the single-slot buffer architecture makes subscription starvation **architecturally impossible** — the real trade-off is message loss under high publish rates, which is by design for bounded-memory `no_std` operation.

## Architecture: Single-Slot Buffer Model

Each subscription and service in nros uses a **single-slot buffer** backed by a static array (`SUBSCRIBER_BUFFERS` / `SERVICE_BUFFERS` in `shim.rs`). The buffer has a boolean ready flag (`has_data` for subscriptions, `has_request` for services) that gates consumption.

**Key invariant:** `try_recv_raw()` and `try_recv_request()` clear the ready flag on **every** code path — success, overflow, and buffer-too-small. This was verified in Phase 37.1 (service fix) and 37.1a (21 buffer state machine tests).

### spin_once() Control Flow

```
spin_once(delta_ms)
  ├── Evaluate trigger (ready mask of has_data/has_request flags)
  │   └── If trigger not satisfied → process timers only, return
  ├── For each node:
  │   ├── process_subscriptions()     [executor.rs:531-538]
  │   │   └── For each sub: while sub.try_process()? { count++ }
  │   ├── process_services()          [executor.rs:543-559]
  │   │   └── For each svc: if svc.try_handle()? { count++ }
  │   └── process_timers(delta_ms)
  └── Return SpinOnceResult { subs, services, timers, errors }
```

## Concern 1: Subscription Starvation from Tight Loop

**Claim (from roadmap):** The `while sub.try_process()?` loop in `process_subscriptions()` (`executor.rs:534`) could starve later subscriptions if topic[0] receives many messages per spin.

**Finding: Architecturally impossible.**

The `while` loop calls `try_process()`, which calls `try_recv()`, which calls `try_recv_raw()` (`shim.rs:1259`). Every path through `try_recv_raw()` clears `has_data` (`shim.rs:1270`, `1278`, `1291`). After the first successful receive (or error), `has_data` is `false`, so the next `try_recv_raw()` returns `Ok(None)`, and `try_process()` returns `Ok(false)`, exiting the `while` loop.

**Result:** Each subscription processes **at most 1 message per `spin_once()`**, regardless of the `while` loop. There is no starvation — all subscriptions get equal opportunity every spin cycle.

The `while` loop exists for future multi-slot buffer support (e.g., ring buffers) but is currently a no-op second iteration for single-slot buffers.

### What Does Happen Under High Publish Rates

If a publisher sends faster than `spin_once()` runs, the zenoh-pico callback overwrites the buffer's contents before the executor can consume them. This is **message loss**, not starvation:

- At 100 Hz publish / 100 Hz spin: ~0% loss (1:1 match)
- At 1000 Hz publish / 100 Hz spin: ~90% loss (9 of 10 messages overwritten)

Both topics on the same executor experience the same loss rate — neither starves the other.

## Concern 2: Service Single-Attempt Limitation

**Claim:** Each service gets only one `try_handle()` per `spin_once()`, causing backlog growth under load.

**Finding: Fair, with single-slot caveat.**

The `process_services()` loop (`executor.rs:545-548`) calls `try_handle()` once per service. This is correct — with a single-slot buffer, there can be at most one pending request per service per spin cycle. A `while` loop (like subscriptions) would exit after one iteration anyway.

The real limitation is the single-slot buffer: if a client sends a request while the server hasn't consumed the previous one, the new request overwrites the old one. However, the service client uses zenoh's **query/reply** mechanism (`client.call()`), which blocks until the response arrives. This serializes requests naturally — no backlog accumulates with a single client.

**Multiple concurrent clients** could cause request loss (last-writer-wins on the single-slot buffer). This is a known trade-off of the bounded-memory architecture, not a fairness issue.

## Concern 3: C API LET Mode vs RCLCPP Mode

**Finding: Both modes are fair. Neither has a starvation issue.**

### RCLCPP Mode (default)

Each subscription's data is fetched immediately before its callback (`nros-c/src/executor.rs:1044-1045`):

```
for each handle:
    if handle is subscription:
        process_subscription(handle)  // calls try_recv + callback
```

This is equivalent to the Rust executor — each subscription gets one attempt per spin.

### LET Mode (Logical Execution Time)

All subscriptions are sampled atomically at the start of `spin_some()` (`nros-c/src/executor.rs:932-934`):

```
sample_all_handles_for_let(executor)  // snapshot all buffers
for each handle:
    if handle is subscription and let_data_available[i]:
        process_subscription_from_let(handle, let_buffer)  // use snapshot
```

LET mode gives **snapshot consistency** — all callbacks see data from the same sampling point. Messages arriving during processing are deferred to the next spin. This is by design for deterministic real-time behavior (LET semantics from AUTOSAR/RTIC).

**Fairness comparison:** Both modes process each subscription exactly once per spin. LET mode samples all data upfront; RCLCPP mode samples just-in-time. Neither starves any subscription.

**Note:** Services are processed identically in both modes — they are NOT pre-sampled in LET mode because they require request-reply semantics (`nros-c/src/executor.rs:882-884`).

## Benchmark Results

**Setup:** Native Linux, zenohd, release build, separate publisher/subscriber processes.
Requires `zenohd --listen tcp/127.0.0.1:7447` running.

Run with: `just bench-fairness`

**Note:** Uses separate processes for publishers and subscribers because zenoh-pico does not deliver self-published messages back to the same session.

### Scenario 1: Asymmetric Subscription Rates

- `/bench1/fast` published at 100 Hz, `/bench1/slow` at 10 Hz
- Executor `spin_one_period()` at 10ms (100 Hz)
- Duration: 10 seconds

**Results:**
- `/bench1/fast`: expected ~1000, received **952** (4.8% loss)
- `/bench1/slow`: expected ~100, received **100** (0% loss)
- Inter-callback intervals: fast p50=10.05ms, p95=10.06ms; slow p50=100.51ms, p95=100.53ms
- Both topics get equal per-spin opportunity — **no starvation observed**

### Scenario 2: Service Request Burst

- 100 requests sent at 10ms intervals from external client (serialized via blocking `call()`)
- Server spin at 10ms interval

**Results:**
- Server handled: **100/100** (100% success rate)
- Blocking `call()` serializes requests naturally — no request loss

### Scenario 3: Mixed Subscription + Service Load

- 2 topics at 50 Hz + 1 service at 10 Hz
- Executor spin at ~100 Hz (10ms)

**Results:**
- `/bench3/topic_a`: expected ~500, received **474** (5.2% loss)
- `/bench3/topic_b`: expected ~500, received **474** (5.2% loss) — **identical** to topic_a
- Service: expected ~100, handled **101**
- No component starves another — **perfect fairness** across subscriptions and services

## Conclusion

**No fairness mitigation (Phase 37.5) is needed.**

The three concerns from the roadmap are resolved:

1. **Subscription starvation:** Impossible with single-slot buffers. The `while` loop runs at most once per subscription per spin.
2. **Service single-attempt:** Correct behavior for single-slot buffers. Blocking `call()` serializes requests naturally.
3. **C LET vs RCLCPP:** Both process each subscription exactly once per spin. LET adds snapshot consistency, not unfairness.

The executor provides **perfect per-spin fairness**: every ready subscription and service gets exactly one processing opportunity per `spin_once()`. The trade-off is message loss under high publish rates, which is inherent to the single-slot buffer design chosen for bounded memory in `no_std` environments.

### Single-Slot Buffer Trade-offs

| Property | Single-slot | Ring buffer (future) |
|----------|------------|---------------------|
| Memory | O(1) per subscription | O(depth) per subscription |
| no_std compatible | Yes | Yes (with heapless) |
| Message loss under load | Yes (overwrite) | Configurable (depth) |
| Fairness per spin | 1 msg/sub/spin | Up to depth msgs/sub/spin |
| Starvation possible | No | Possible without caps |

The single-slot design is appropriate for the target embedded systems (Cortex-M3, ESP32-C3, STM32F4) where memory is severely constrained and deterministic timing is more important than guaranteed delivery.

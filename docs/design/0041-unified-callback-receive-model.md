---
rfc: 0041
title: "Unified callback + QoS-depth receive model for clients (service/action)"
status: Draft
since: 2026-06
last-reviewed: 2026-06-12
implements-tracked-by: [phase-238]
supersedes: []
superseded-by: null
---

# RFC-0041 — Unified callback + QoS-depth receive model

## Summary

Subscriptions deliver received messages through an executor-driven **callback**
(`FnMut(&M)`) backed by a QoS-depth **`BufferStrategy`** (triple-buffer at
depth ≤ 1, SPSC ring at depth > 1, with a `MessageLost` signal). Service and
action **clients** do not: they receive a reply / result / feedback through a
poll-based `Promise` over a **single reusable buffer**, drained only when the
user calls `Promise::try_recv()`.

That asymmetry has two costs. (1) **Reliability:** a single reply/feedback
buffer silently overwrites an un-consumed message when a second arrives — which
also violates the ROS service QoS contract (`KEEP_LAST(10)`, RFC-0007). (2)
**Consistency / RT:** users juggle two paradigms, and on poll-based transports
`Promise::wait()` can burn its timeout budget (the `zpico` 39 ms pitfall).

This RFC defines **one receive model** for every callback-capable entity —
subscription, service server, service client, action goal/feedback/result —
built on the existing subscription machinery: an executor-dispatched **callback**
fed by a **QoS-depth `BufferStrategy`**, drained once per `spin_once` after the
single transport pump. The `Promise` API is **retained** as the future-style
option (dual-mode), so no existing call site breaks.

This converges nano-ros toward the ROS 2 client libraries (rclcpp
`async_send_request(req, cb)` + action `SendGoalOptions{goal_response_callback,
feedback_callback, result_callback}`) while preserving the deliberate non-blocking
`Promise` divergence (RFC-0036, RFC-0021).

## Motivation / problem

### The current asymmetry

| Entity | Today | Buffer | Loss on burst |
|---|---|---|---|
| Subscription | callback `FnMut(&M)`, drained at spin | `BufferStrategy(depth)` | bounded + `MessageLost` |
| Service server | callback, drained at spin | request/reply buffers | n/a (req→reply synchronous) |
| **Service client (reply)** | `Promise::try_recv()` poll | **single `reply_buffer`** | **silent overwrite** |
| **Action client (feedback)** | `try_recv_feedback()` poll | **single `feedback_buffer`** | **silent overwrite** |
| **Action client (result)** | `Promise` poll | **single result buffer** | **silent overwrite** |

The client single-buffer overwrite is a real bug: if two replies/feedbacks
arrive between polls, the first is lost with no signal — unlike the subscription
ring's `MessageLost`. ROS service QoS is `RELIABLE + VOLATILE + KEEP_LAST(10)`
(RFC-0007); a single slot cannot honor a depth-10 history.

### ROS 2 alignment

rclcpp delivers client receives through **callbacks** as a first-class option:
`Client::async_send_request(request, response_callback)` and
`Client::send_goal(goal, SendGoalOptions{goal_response_callback,
feedback_callback, result_callback})`. rclc's executor is callback-only. nano-ros
already mirrors this for subscriptions and servers; extending it to clients is
*convergence*, not divergence. The `Promise` stays as the spin-driven
future-equivalent (RFC-0036 row: nano-ros diverged from rclcpp's **blocking**
`.get()`, not from callbacks — this RFC keeps that divergence and adds the
callback option rclcpp also offers).

## Design

### One model: pump → drain(QoS buffer) → dispatch(callback)

`spin_once` already has the right shape (`executor/spin.rs`):

```
spin_once(timeout):
  1. session.drive_io(timeout_ms)          // pump transport ONCE per session
     extra_sessions: drive_io(0)
  2. for entry in entries: bits |= has_data(entry)   // non-blocking readiness
  3. trigger evaluation
  4. for entry in entries if ready: try_process(entry, dt)  // drain buffer → callback
```

Step 1 pumps the transport once. For a **poll-based backend (XRCE)** that single
`drive_io` runs `uxr_run_session_time`, whose internal callback fills each reply
slot's `has_reply` flag; `try_recv_reply_raw` is then a pure non-blocking
flag+copy. For a **wake-capable backend (zenoh-pico, Cyclone)** `drive_io`
unblocks on data arrival. **Both feed the same buffer→dispatch path** — the
callback model is transport-agnostic because the pump is per-session, not
per-entity.

This RFC makes service/action **client** receives reuse steps 2–4 exactly as
subscriptions do today: the RMW drain at spin is the **producer** into a
`BufferStrategy`, the dispatch loop is the **consumer** that invokes the user
callback.

### Buffering — QoS depth drives `BufferStrategy` (unified)

Every callback receive uses the existing strategy, keyed on the entity's QoS
`depth`:

- **depth ≤ 1** → `TripleBuffer` (latest-value, lock-free, 3 slots).
- **depth > 1** → `SpscRing(depth)` (FIFO, drop-newest on overflow, `MessageLost`
  reported).

Applied to clients:

| Client receive | Default QoS | Buffer |
|---|---|---|
| Service reply | services_default `KEEP_LAST(10)` | `SpscRing(10)` |
| Action result | services_default | ring / triple per QoS |
| Action feedback | feedback-topic QoS | ring / triple per QoS |
| Action goal-response | services_default | ring / triple per QoS |

This honors ROS `KEEP_LAST(10)` for services and fixes the silent-overwrite bug
uniformly. Correlated one-shot receives (a single outstanding request) simply
rarely fill their ring — correct, with no special-case code.

### Dual-mode API (Promise retained)

`Promise` stays for every client (≈ 90+ call sites unaffected). New callback
registrations are **added**, mirroring `create_subscription`:

```rust
// Service client — rclcpp async_send_request(req, cb) analogue.
node.create_client_with_callback::<Svc, _>(client, |reply: &Svc::Reply| { … });

// Action client — rclcpp SendGoalOptions analogue.
node.create_action_client_with_callbacks::<A, _, _, _>(
    client,
    on_goal_response: |id: &GoalId, accepted: bool| { … },
    on_feedback:      |id: &GoalId, fb: &A::Feedback| { … },
    on_result:        |id: &GoalId, st: GoalStatus, r: &A::Result| { … },
);
```

These wrap the **already-present** C-API arena entries
(`ActionClientRawArenaEntry`, `ServiceClientRawArenaEntry` + their
`*_raw_try_process` eager-drain dispatchers) with a monomorphised typed
`try_process` that deserializes from the buffer and invokes the Rust closure —
the same wrapping pattern Phase 235 / subscriptions use. The C API already
exposes the raw form; this RFC adds the typed Rust surface + the QoS buffer.

**Correlation.** Action callbacks already carry `goal_id` (extracted from the
CDR payload / goal counter). Service replies carry raw bytes; the typed callback
deserializes the reply and the user correlates by content (rclcpp's
`async_send_request` callback likewise binds the response to the request via the
returned future/closure capture, not a wire field).

### Real-time constraints (RFC-0002)

The hot path must preserve the RT contract (single-thread, non-preemptive,
run-to-completion, no heap, no OS mutex, bounded per callback):

1. **Transport pump once per `spin_once` per session** — never per entity.
2. **Buffers are lock-free SPSC, no-alloc, O(1)** — `TripleBuffer` / `SpscRing`
   are already proven RT-clean; producer = RMW drain at spin, consumer = dispatch,
   **same thread**.
3. **Callbacks are `FnMut(&Msg)`** over a buffer-resident message — no heap in the
   signature; user callbacks must not block or allocate (same contract as
   subscription callbacks).
4. **No new locks / no priority inversion.** A service-server callback that calls
   a client with a callback runs in the same tick on the same thread → safe; the
   buffer decouples the send from the eventual callback (no blocking send inside a
   callback).
5. **Prefer callbacks over `Promise::wait()` on poll-based transports.** Callback
   delivery fires at `drive_io` return, avoiding the `Promise::wait()` budget-burn
   on XRCE/zpico (CLAUDE.md pitfall). The RFC recommends callbacks for hard-RT over
   XRCE; `Promise` remains valid for soft-RT / request-reply ergonomics.

### Backend parity

| Backend | Receive primitive | Pump model | Callback model |
|---|---|---|---|
| XRCE (poll) | `try_recv_*_raw` flag+copy | explicit `uxr_run_session_time` loop in one `drive_io` | ✅ via per-spin pump → buffer → dispatch |
| zenoh-pico | `try_recv_*_raw` | `drive_io` unblocks on wake callback | ✅ |
| Cyclone | `try_recv_*_raw` | wake callback / participant | ✅ |

No backend change is required — the receive primitives are identical to the
subscription path; only the node layer changes.

## Alternatives considered

- **Replace `Promise` with callbacks (breaking).** Rejected — ~90 call sites,
  and the spin-driven `Promise` is a deliberate, valuable RFC-0021 primitive for
  request-reply ergonomics + RTOS-bounded waits. Dual-mode keeps both.
- **Single-buffer + waker only (no ring).** Cheapest, but a burst between two
  spins still overwrites silently and cannot honor `KEEP_LAST(10)`. Rejected —
  fails the reliability motivation; the QoS-depth ring is the ROS-correct unit.
- **Ring everywhere regardless of QoS.** Rejected — depth is already the right
  knob; depth-1 receives use the cheaper triple-buffer, matching subscriptions.

## RT / reliability acceptance

- Two replies/feedbacks arriving between spins on a depth>1 client are **both
  delivered** (or the overflow reported via `MessageLost`) — no silent loss.
- Service-client default QoS buffers `KEEP_LAST(10)` (RFC-0007 honored).
- Callback hot path adds **no heap allocation and no lock** vs the subscription
  path (verified against `wcet-cycles` / `wake-latency` benches).
- One `drive_io` per `spin_once` per session across XRCE / zenoh / Cyclone.
- Existing `Promise` call sites compile + pass unchanged (dual-mode).

## References

- RFC-0002 (RT execution model — the hot-path contract).
- RFC-0007 (service/action QoS — `KEEP_LAST(10)` default).
- RFC-0021 / RFC-0036 (spin-driven `Promise`; the deliberate non-blocking
  divergence this RFC preserves).
- RFC-0015 (priority tiers / reentrant groups — same-thread callback safety).
- RFC-0037 (user API surface — adds the `create_*_with_callback` entries).
- Code: `executor/spin.rs` (`drive_io` + dispatch), `executor/arena.rs`
  (`ActionClientRawArenaEntry`, `ServiceClientRawArenaEntry`,
  `*_raw_try_process`), `executor/{triple_buffer,spsc_ring}.rs`,
  `nros-rmw-xrce/src/{service.c,session.c}` (poll pump).
- Phase 238 (implementation).

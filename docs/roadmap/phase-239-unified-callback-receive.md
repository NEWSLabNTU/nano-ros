# Phase 239 — Unified callback receive model for clients (Rust)

**Goal.** Implement [RFC-0041](../design/0041-unified-callback-receive-model.md):
bring service/action **client** receives (reply / result / feedback / goal-response)
up to the subscription model — executor-dispatched **callbacks** fed by a QoS-depth
**`BufferStrategy`** — while keeping the `Promise` API (dual-mode). Rust first;
C/C++ follow in a later phase. Fixes the silent single-buffer overwrite and honors
ROS service `KEEP_LAST(10)` (RFC-0007).

**Status.** In progress (2026-06). **Wave 1 complete** — 239.1 (service-client) +
239.2 (action-client) typed callbacks + 239.3 wiring + 239.4 in-process E2Es
(`test_{service,action}_client_callback*`, 161 nros-node tests green). Next:
Wave 2 (QoS-depth buffering), Wave 4 (C/C++). Implements RFC-0041.

**Priority.** P2 — reliability + RT-ergonomics + ROS alignment; not a correctness
blocker (Promise works today) but removes a real silent-loss bug.

**Depends on.** RFC-0041 (design), RFC-0002 (RT hot-path contract), RFC-0007
(service QoS default), RFC-0021/0036 (Promise — preserved). Reuses the existing
C-API arena machinery: `ActionClientRawArenaEntry` / `ServiceClientRawArenaEntry`
+ `action_client_raw_try_process` / `service_client_raw_try_process`
(`executor/arena.rs`), the `register_action_client_raw_sized` registration
(`executor/action.rs`, already public), and `TripleBuffer` / `SpscRing` /
`BufferStrategy` (`executor/{triple_buffer,spsc_ring}.rs`).

## Overview

The runtime shape already exists: `spin_once` pumps the transport once per session
(`session.drive_io`, `executor/spin.rs:4054`), then readiness-scans + dispatches
each arena entry's `try_process`. The C API already eager-drains client receives
into callbacks there. This phase adds the **typed Rust** surface over that
machinery (wave 1), then swaps the client single buffers for the QoS-depth
`BufferStrategy` so bursts are buffered/reported instead of overwritten (wave 2).
`Promise` is untouched — both paths coexist.

## Architecture

```
spin_once:
  drive_io(timeout)  ── XRCE: uxr_run_session_time fills reply slots (poll)
                        zenoh/cyclone: unblock on wake
        │
        ▼  (per entry, once/spin)
  has_data?  ─► try_process ─► [QoS BufferStrategy] ─► deserialize ─► FnMut(&Msg)
                 producer = RMW drain          consumer = dispatch (same thread, RT)
```

## Work Items

### Wave 1 — Rust typed callback API (dual-mode, on today's buffers)

#### 239.1 — Service-client callback registration  ✅ (code; runtime test → 239.4)
Add `NodeCtx::create_client_with_callback::<Svc, F>(client, callback)` where
`F: FnMut(&Svc::Reply) + 'static`. Wrap a new typed arena entry
(`ServiceClientCallbackEntry<Svc, F>`) over the existing
`ServiceClientRawArenaEntry` pattern: a monomorphised `try_process` that drains
`try_recv_reply_raw` → `CdrReader` → `Svc::Reply::deserialize` → invokes the
closure. Reuse the `reply_ready` waker gate. `Promise` path unchanged.
- **Files:** `executor/handles.rs`, `executor/arena.rs`, `executor/spin.rs`
  (registration), `executor/node.rs` (`create_client_with_callback`).

#### 239.2 — Action-client callbacks  ✅ (code; runtime test → 239.4)
Add `NodeCtx::create_action_client_with_callbacks::<A, …>(client, on_goal_response,
on_feedback, on_result)` with `FnMut(&GoalId, bool)` / `FnMut(&GoalId, &A::Feedback)`
/ `FnMut(&GoalId, GoalStatus, &A::Result)`. Wrap `ActionClientRawArenaEntry` (which
already carries the three raw callbacks + `goal_id` extraction) with typed
trampolines that deserialize the payload then call the closures. `register_action_
client_raw_sized` is already public — add the typed wrapper.
- **Files:** `executor/handles.rs`, `executor/action.rs`, `executor/node.rs`.

#### 239.3 — Registration + executor wiring  ✅ (done in 239.1/239.2 registrations)
Hook the new typed entries into the `CallbackMeta` list (`EntryKind::ServiceClient`
/ `ActionClient`, `InvocationMode::Always`, the typed `try_process` / `has_data` /
`drop_fn`), mirroring `register_subscription_buffered_on`. Confirm one `drive_io`
per spin still pumps the session for all entries (no per-entity pump).
- **Files:** `executor/spin.rs`.

#### 239.4 — Wave-1 tests  ✅
Native tests: a callback fires at `spin_once` for service reply + action
feedback/result/goal-response (no `Promise::try_recv`); `Promise` + callback
coexist (dual-mode) without interfering. Assert the callback runs in the spin
thread.
- **Files:** `packages/testing/nros-tests/tests/` (native_api / a new
  `client_callbacks` test).

### Wave 2 — QoS-depth buffering (reliability)

**Scope refinement (2026-06).** The burst hazard only exists where multiple
messages can arrive before one is consumed. **Service reply** and **action
result** are *single-outstanding* (gated by `pending` — a second request can't be
sent until the first is answered), so they hold ≤ 1 in flight and a ring adds no
demonstrable benefit; they keep their gated single buffer. The real stream is
**action feedback** — that gets a ring. Crucially, the callback entry gets its
**own** feedback ring (drain `core.feedback_subscriber` → ring), so the shared
`ActionClientCore` buffers (used by the `Promise` path) are **not** touched.

#### 239.5 — Action-feedback ring on the callback entry  ✅
Add a `feedback_buffer: BufferStrategy` to `ActionClientCallbackEntry` (trailing-
allocated; `SpscRing(depth)` for depth > 1, `TripleBuffer` for depth ≤ 1). The
feedback phase of `action_client_callback_try_process` drains
`core.feedback_subscriber` directly into the ring (replicating the goal-id +
payload-offset extraction), then pops + deserializes `A::Feedback` per slot.
Goal-response / result keep the core's gated single buffer.
- **Files:** `executor/arena.rs`, `executor/action.rs` (registration trailing
  alloc), `executor/node.rs` (feedback QoS depth).

#### 239.6 — `MessageLost` on feedback-ring overflow  ⬜
On feedback ring overflow, signal `MessageLost` (mirror the subscription
`on_message_lost`). Feedback uses its topic QoS depth; service reply / action
result honor `services_default` (`KEEP_LAST(10)`, RFC-0007) on the gated buffer
(≤ 1 in flight, so a no-op in practice — documented).
- **Files:** `executor/arena.rs` / `executor/handles.rs` (lost signal).

#### 239.7 — Wave-2 reliability test  ✅
Burst test: two feedbacks arrive between spins on a depth > 1 action callback
client → **both delivered** (vs the pre-239 single-buffer overwrite). A depth-1
client coalesces to latest (triple-buffer). In-process `MockSession` E2E extending
`test_action_client_callbacks_fire_at_spin`.
- **Files:** `packages/core/nros-node/src/executor/tests.rs`.

### Wave 3 — RT + backend validation

#### 239.8 — RT hot-path + XRCE poll validation  ⬜
- Confirm the callback dispatch adds **no heap alloc, no lock** vs the
  subscription path (RFC-0002) — check with `nros-bench/wcet-cycles-qemu` /
  `wake-latency`.
- Verify XRCE: one `drive_io` per spin pumps the session; callbacks fire without
  `Promise::wait` (no budget-burn). Run a callback client over the XRCE backend.
- Verify zenoh-pico + (if available) Cyclone parity.
- **Files:** none (validation); fixes land in the relevant wave if a gap surfaces.

#### 239.9 — Example  ⬜
A callback-based service-client (and/or action-client) example mirroring an
existing Promise example, showing the dual-mode surface.
- **Files:** `examples/<plat>/rust/…`.

### Wave 4 — C / C++ callback clients (service + action)

C/C++ deliver the same callback model, reusing the Rust FFI (project principle:
**C++ wraps Rust; C uses the raw arena callbacks**). The raw arena entries
(`ServiceClientRawArenaEntry`, `ActionClientRawArenaEntry`) + their registrations
already eager-drain at spin and invoke C-ABI callbacks (`RawResponseCallback`,
`RawGoalResponseCallback`, `RawFeedbackCallback`, `RawResultCallback`) — so the C
runtime path largely **exists**; this wave exposes the ergonomic surface, adds the
C++ typed wrappers, and proves both end-to-end.

#### 239.11 — C service-client callback surface + E2E  ⬜
Expose a clean C API over the raw response callback: `nros_client_init` +
`nros_client_set_response_callback` (or an init-with-callback) + `nros_client_send_request`;
the callback receives raw reply bytes which the user feeds to the generated
`{Svc}_Response_deserialize`. E2E: a native C service server + callback client
round-trip (assert the callback fires at spin, no poll).
- **Files:** `packages/core/nros-c/src/service.rs`, `nros-c/include/nros/client.h`,
  a C fixture under `packages/testing/` or `examples/*/c/`.

#### 239.12 — C action-client callback surface + E2E  ⬜
Same over `ActionClientRawArenaEntry`'s goal-response / feedback / result raw
callbacks (already wired in the C API): confirm/expose `nros_action_client_init` +
the three callback setters + `send_goal` / `get_result`. E2E: native C action
server + callback client.
- **Files:** `packages/core/nros-c/src/action.rs`, `nros-c/include/nros/action_client.h`,
  a C fixture.

#### 239.13 — C++ service-client callback wrapper + E2E  ⬜
C++ wraps the Rust FFI: `nros::Client<Svc>::async_send_request(req, callback)` that
registers the raw callback + deserializes the reply via the generated
`ffi_deserialize` into `Svc::Response`, then invokes the typed C++ closure —
mirroring the C++ subscription callback (Phase 235 pattern). E2E: native C++ server
+ callback client (spans/owned).
- **Files:** `packages/core/nros-cpp/include/nros/client.hpp`, cpp codegen if a
  per-type trampoline is needed, a C++ fixture.

#### 239.14 — C++ action-client callback wrapper + E2E  ⬜
`nros::ActionClient<A>` with `goal_response` / `feedback` / `result` callbacks
wrapping the raw FFI + generated deserialize. E2E: native C++ action server +
callback client.
- **Files:** `packages/core/nros-cpp/include/nros/action_client.hpp`, a C++ fixture.

#### 239.15 — Cross-language E2E matrix  ⬜
Callback-client interop across Rust / C / C++ (each language's callback client
against another language's server), native + one QEMU/embedded lane, to prove the
callback receive model is wire-compatible and backend-agnostic (zenoh + XRCE).
- **Files:** `packages/testing/nros-tests/tests/` (a `client_callbacks_interop`
  harness), reusing the existing cross-RMW fixture matrix.

### Close-out

#### 239.10 — Docs sync  ⬜
Tick RFC-0037 (user API surface — `create_client_with_callback` /
`create_action_client_with_callbacks` + the C/C++ surfaces); flip RFC-0041 →
`Stable` once Rust + C/C++ land. (C/C++ are now in-phase — Wave 4 — not a deferred
follow-up.)
- **Files:** `docs/design/0037-*`, `docs/design/0041-*`.

## Acceptance

- Service/action clients deliver reply/result/feedback/goal-response via Rust
  closures dispatched at `spin_once`; `Promise` still works (dual-mode), all
  existing call sites unchanged.
- A burst of two messages between spins on a depth>1 client is buffered/reported,
  not silently overwritten; service-client default honors `KEEP_LAST(10)`.
- One `drive_io` per spin per session across XRCE / zenoh / Cyclone; callbacks
  fire on the poll-based XRCE path without `Promise::wait` budget-burn.
- No new heap alloc / lock in the dispatch hot path (RFC-0002).
- **C and C++** callback service/action clients deliver typed receives at spin
  (C: raw callback + generated deserialize; C++: typed wrapper over the Rust FFI),
  each with a native E2E + a cross-language interop test (zenoh + XRCE).
- `just ci` green. RFC-0041 → `Stable` once Rust + C/C++ land.

## Notes

- **Dual-mode is load-bearing:** ~90 `Promise` call sites must keep compiling.
  Do not gate the `Promise` path behind the callback path; they share the RMW
  receive primitive but are independent consumers.
- **Correlation:** action callbacks carry `goal_id` already; service-reply
  callbacks deserialize the reply and the user correlates by content (rclcpp
  parity — the response callback binds to the request via the closure, not a wire
  field).
- **RT recommendation (RFC-0041):** prefer callbacks over `Promise::wait` on
  poll-based transports (XRCE/zpico) for hard-RT — callbacks fire at `drive_io`
  return, avoiding the budget-burn pitfall.

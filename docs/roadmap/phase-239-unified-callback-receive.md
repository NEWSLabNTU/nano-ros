# Phase 239 — Unified callback receive model for clients (Rust)

**Goal.** Implement [RFC-0041](../design/0041-unified-callback-receive-model.md):
bring service/action **client** receives (reply / result / feedback / goal-response)
up to the subscription model — executor-dispatched **callbacks** fed by a QoS-depth
**`BufferStrategy`** — while keeping the `Promise` API (dual-mode). Rust first;
C/C++ follow in a later phase. Fixes the silent single-buffer overwrite and honors
ROS service `KEEP_LAST(10)` (RFC-0007).

**Status.** In progress (2026-06). **Wave 1 complete** (239.1-4: both client
callbacks + in-process E2Es). **Wave 2 core done** — 239.5 (action-feedback
QoS-depth ring) + 239.7 (burst test: 2 feedbacks both delivered). 162 nros-node
tests green. 239.6 resolved (descope — MessageLost is an RMW event, not ring overflow);
239.8 RT/XRCE validated by inspection. 239.9 (native callback example) done.
**Wave 4 audit:** C *and* C++ callback surfaces (service + action) all already
exist from Phase 189.M3.3 — Wave 4 ships no new wrapper code; remaining is E2E
fixtures (C + C++) + cross-language matrix (239.15). Implements RFC-0041.

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

#### 239.6 — `MessageLost` signal  ✅ (descoped — architecture clarification)
**Finding.** `MessageLost` is an **RMW transport event** (DDS lost-sample,
surfaced via `Subscription::on_message_lost` / the backend event queue), *not* a
ring-overflow condition. The feedback ring **defers** rather than loses: when the
ring is momentarily full the drain loop stops and the remaining messages stay in
the subscriber to be drained next spin. True loss happens only if the *transport*
buffer overflows, which the RMW already reports as a `MessageLost` event. So
there is no ring-overflow signal to add. The genuine follow-up is exposing the
feedback subscriber's RMW `MessageLost` event on `ActionClientCallback` (e.g.
`on_feedback_message_lost`), mirroring `Subscription::on_message_lost` — deferred
to the C/C++ wave / a later increment since it is a transport-event passthrough,
not part of the buffering reliability (already delivered in 239.5/7).

#### 239.7 — Wave-2 reliability test  ✅
Burst test: two feedbacks arrive between spins on a depth > 1 action callback
client → **both delivered** (vs the pre-239 single-buffer overwrite). A depth-1
client coalesces to latest (triple-buffer). In-process `MockSession` E2E extending
`test_action_client_callbacks_fire_at_spin`.
- **Files:** `packages/core/nros-node/src/executor/tests.rs`.

### Wave 3 — RT + backend validation

#### 239.8 — RT hot-path + XRCE poll validation  🟡 (inspection ✅; QEMU benches → CI)
- **No heap alloc, no lock (RFC-0002) — verified by inspection.** The three new
  dispatchers (`service_client_callback_try_process`,
  `action_client_callback_try_process`, `dispatch_feedback`) use only stack
  buffers + a stack `CdrReader`, the **lock-free SPSC** `BufferStrategy`
  (`TripleBuffer` / `SpscRing`, already proven RT-clean), and the user closure —
  no `alloc`, no mutex, single-thread, run-to-completion. Same hot-path shape as
  the subscription dispatch.
- **XRCE poll — structurally confirmed.** `spin_once` pumps **one `drive_io` per
  session** (`executor/spin.rs:4054`) before the per-entry drain; the callback
  entries are `InvocationMode::Always` and drain non-blocking
  (`try_recv_reply_raw` / `try_recv_raw` flag+copy), so they fire at `drive_io`
  return with no `Promise::wait` budget-burn — the model is transport-agnostic
  (RFC-0041 backend-parity table).
- **Deferred to CI:** the `wcet-cycles-qemu` / `wake-latency` numbers + a live
  callback client over XRCE / zenoh-pico / Cyclone (needs the QEMU lanes).
- **Files:** none (validation).

#### 239.9 — Example  ✅ (service-client; action-client → Wave 4)
A callback-based service-client (and/or action-client) example mirroring an
existing Promise example, showing the dual-mode surface.
- **Files:** `examples/<plat>/rust/…`.

### Wave 4 — C / C++ callback clients (service + action)

C/C++ deliver the same callback model, reusing the Rust FFI (project principle:
**C++ wraps Rust; C uses the raw arena callbacks**). The raw arena entries
(`ServiceClientRawArenaEntry`, `ActionClientRawArenaEntry`) + their registrations
already eager-drain at spin and invoke C-ABI callbacks (`RawResponseCallback`,
`RawGoalResponseCallback`, `RawFeedbackCallback`, `RawResultCallback`) — so the C
runtime path **already exists and is wired** for both service (239.11) and action
(239.12) — Phase 189.M3.3 et al. **Update (Wave 4 audit):** the C++ typed wrappers
(239.13/14) **also already exist** from the same phase — `async_send_request` +
`response_trampoline` (client.hpp) and `SendGoalOptions` + `set_callbacks` /
`register_callbacks` (action_client.hpp). So Wave 4 ships **no new wrapper code** —
its only remaining work is the **E2E fixtures** (C + C++) and the **cross-language
E2E matrix** (239.15).

#### 239.11 — C service-client callback surface + E2E  ✅ (E2E GREEN)
**Done.** Added `examples/native/c/service-client-callback` (registers
`nros_client_set_response_callback`, sends with `nros_client_send_request_async`,
replies dispatched at `nros_executor_spin_some`). E2E
`test_native_service_communication_callback::C` (native_api.rs) pairs it with the
stock C service server — 4/4 replies delivered via the callback (no poll).

Original finding (still accurate):
**Finding.** The C surface already exists + is wired (Phase 189.M3.3):
`nros_client_set_response_callback` (`service.rs:1437`) +
`nros_client_send_request_async` (`service.rs:1701`); the registration
(`executor.rs:1153` → `register_service_client_raw_sized_on`) installs the raw
`RawResponseCallback` arena entry that the executor drains at spin. The callback
receives raw reply bytes → user deserializes with the generated
`{Svc}_Response_deserialize`. **Remaining:** a native E2E (C service server +
C callback client; assert the callback fires at spin, no poll) + a doc/example.
- **Files:** a C fixture/example under `examples/*/c/` or `packages/testing/`.

#### 239.12 — C action-client callback surface + E2E  🟡 (surface ✅ already; E2E ⬜)
**Finding.** Already wired: `nros_action_client_set_goal_response_callback` /
`_set_feedback_callback` / `_set_result_callback` + `register_action_client_raw`
(`executor.rs:1404-1494`) install the `ActionClientRawArenaEntry`'s three raw
callbacks, drained at spin. **Remaining:** a native C action server + callback
client E2E + example.
- **Files:** a C fixture/example.

#### 239.13 — C++ service-client callback wrapper + E2E  ✅ (E2E GREEN; **bug fixed**)
**Done — but the wrapper was broken.** The C++ surface existed (Phase 189.M3.3.f:
`create_client(out, name, callback, …)` + `response_trampoline`, `async_send_request`)
but **never delivered replies**: `nros_cpp_service_client_send_on_handle` called
`send_request_raw` without setting the arena entry's `pending` flag, and
`service_client_raw_try_process` early-returns unless `pending` is set — so the
reply arrived but was never dispatched. **Fixed** to mirror the C wrapper exactly
(clear `reply_ready`, send, set `pending = true`, register the reply waker). E2E
`test_native_service_communication_callback::Cpp` now GREEN (4/4 via callback).

Also added `examples/native/cpp/service-client-callback`. It uses `nros::init()`
(env-var fallback) rather than `init_with_launch_auto`, which has a latent
null-locator bug → issue #39.
- **Files:** `packages/core/nros-cpp/src/service.rs` (the fix),
  `examples/native/cpp/service-client-callback/`, `native_api.rs`.

#### 239.14 — C++ action-client callback wrapper + E2E  🟡 (wrapper ✅ already; E2E ⬜)
**Finding.** Already exists (Phase 189.M3.3.f): `action_client.hpp` has
`SendGoalOptions{goal_response, feedback, result, context}` (line 263) +
`ActionClient<A>::set_callbacks(options)` (line 316) wiring
`nros_cpp_action_client_register_callbacks` (line 42) with three typed trampolines
(goal-response/feedback/result) that `ffi_deserialize` the bytes before the typed
closure. **Remaining:** a native C++ action server + callback client E2E.
- **Files:** C++ fixture (wrapper already in
  `packages/core/nros-cpp/include/nros/action_client.hpp`).

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

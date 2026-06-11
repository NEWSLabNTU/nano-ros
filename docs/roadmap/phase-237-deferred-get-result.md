# Phase 237 — Deferred `get_result` (seq-keyed service reply, concurrent-safe)

**Goal.** Make a nano-ros action **server** answer a `get_result` request only when
the goal reaches a terminal state — the semantics `rclcpp_action` requires — instead
of replying immediately with the goal's live status. This is the remaining 233.6
action-interop item: the forward direction (`ros2 action send_goal` → nano-ros XRCE
server) accepts + streams feedback, but `ros2` waits forever for the final *result*.

**Approach (chosen): Option A — robust.** Honor the `ServiceServerTrait` contract that
`sequence_number` is the request→reply correlation token and a reply may happen any
time after the handler returns. Concurrent goals can occur under heavy load, so the
single-in-flight shortcut (Option C) is rejected.

**Status.** ◐ Implemented across all backends: runtime (237.1) + XRCE (237.3) +
Cyclone-verify (237.2) + Zenoh (237.4), validated against real `rmw_fastrtps`
ROS 2 (forward action delivers the final result) plus backend-agnostic concurrent
unit tests (237.5). Remaining: a concurrent-goals **e2e** over a live transport
and `rmw_zenoh_cpp` interop validation (237.5 tail). Implements the "REMAINING —
get_result deferral" item in [phase-233](phase-233-px4-xrce-companion.md). Off the
PX4 critical path (PX4 is topic-only).

**Depends on.** RFC-0035 (CFFI vtable), the action runtime in
`packages/core/nros-node/src/executor/action_core.rs`, the three service backends
(`nros-rmw-xrce`, `nros-rmw-zenoh`, `nros-rmw-cyclonedds`).

## Problem

`rclcpp_action` sends the `get_result` request **immediately after goal acceptance**
and expects the reply only once the goal terminates. nano-ros'
`ActionServerCore::try_handle_get_result_raw` replies right away:

- goal in `completed_results` → reply with stored result ✅
- goal **active** → reply now with live status + default result ❌ — the bug
- goal unknown → reply now (error) ✅

nano-ros↔nano-ros doesn't hit this: its client sends `get_result` only after seeing
the goal terminate (status topic), so the `completed_results` branch always runs.

## The correlation contract (the hard part)

`ServiceServerTrait` documents: "long handlers should dispatch work to a worker queue
and **reply later via the recorded `sequence_number`**." The backends honor this
unevenly:

| Backend | request buffering | reply token | `send_reply` keys on | deferral today |
| --- | --- | --- | --- | --- |
| **Cyclone** (`service.cpp`) | `slots[kRequestSlots=32]` | `(guid, seq)` RTPS header in slot | **slot index** (returned as `seq`) | **works** (≤32 in-flight) |
| **XRCE** (`service.c`) | single inbox slot | `slot->sample_id` (24 B) | nothing — `(void)seq`, uses the one slot | breaks on a 2nd request |
| **Zenoh** (`zpico.c` + `shim/service.rs`) | single | `g_stored_queries[handle]` (one owned query/queryable) | nothing — `seq` made in the Rust shim, unlinked | breaks on a 2nd request |

**Cyclone is the reference pattern**: `try_recv_request` stashes the request header
into a free persistent slot and returns the slot index as `sequence_number`;
`send_reply(seq)` looks up `slots[seq]`, builds the reply, frees the slot. XRCE and
Zenoh must converge on this "slot-index-as-seq, persistent reply-slot table" shape.

## Work items

### 237.1 — Runtime deferral (shared, all backends)
`packages/core/nros-node/src/executor/action_core.rs`.
- Add `pending_get_results: heapless::Vec<PendingGetResult, MAX_GOALS>` to
  `ActionServerCore`, `PendingGetResult { goal_id: GoalId, sequence_number: i64 }`.
- `try_handle_get_result_raw` else-branch: goal **active** (in `active_goals`) → push
  `(goal_id, seq)`, return **without** replying; **unknown** → reply immediately
  (unchanged).
- `complete_goal_raw`: after storing the result, drain `pending_get_results` entries
  matching `goal_id` → `get_result_server.send_reply(seq, status + result_cdr)` each,
  remove them. Factor the reply build (`i8` status + `align(4)` + result CDR) into a
  helper shared with the `completed_results` branch.
- Construction sites — add `pending_get_results: heapless::Vec::new()` to the 4 literal
  `ActionServerCore { … }` (`executor/action.rs` ×2, `executor/node.rs` ×2) and
  `from_channels` (used by `nros-c` / `nros-cpp`). No per-language logic change — C/C++
  call the same core.
- Overflow: `pending_get_results` full → fail-loud (`NodeError`).

### 237.2 — Cyclone: verify (no functional change expected)
`packages/dds/nros-rmw-cyclonedds/src/service.cpp` already returns the slot index as
`seq` and keys `send_reply` on it. Confirm: (a) the slot is freed only on `send_reply`
(not on `try_recv_request`), so a deferred reply still finds it; (b) `try_recv_request`
returns `WOULD_BLOCK` when all 32 slots are in use (back-pressure, runtime retries next
spin). Add a unit/e2e covering an interleaved 2-request deferral.

### 237.3 — XRCE: seq-keyed reply-token table
`packages/xrce/nros-rmw-xrce/src/{internal.h,service.c}`.
- Add `struct { SampleIdentity sample_id; bool in_use; } reply_slots[XRCE_MAX_PENDING_REPLIES]`
  to `xrce_service_server_state` (new `#define XRCE_MAX_PENDING_REPLIES 4`, overridable).
- `xrce_service_try_recv_request`: copy `slot->sample_id` into a free `reply_slots[i]`,
  set `in_use`, return `i` via `*seq_out` (was hard-coded `0`). `WOULD_BLOCK` if the
  table is full.
- `xrce_service_send_reply(seq, …)`: use `reply_slots[seq].sample_id` for
  `uxr_buffer_reply`; clear `in_use` after. (Drop the `(void)seq;`.)
- The single request *inbox* stays (drained each spin). Optional hardening: multi-entry
  inbox for request bursts faster than the spin rate — separable, not required for
  deferral.

### 237.4 — Zenoh: seq-keyed reply-query table (the real work) ✅ implemented
**Done:** `g_stored_queries` is now `[ZPICO_MAX_QUERYABLES][ZPICO_MAX_PENDING_REPLIES]`
of owned cloned queries; `query_handler` allocates a free slot and records its
index in `g_last_reply_seq[handle]`; the new `zpico_queryable_take_reply_seq`
getter (called from inside the synchronous callback) hands that index to the Rust
`queryable_callback`, which records it as the request `sequence_number` (replacing
the free-running `SERVICE_SEQ_COUNTER`); `zpico_query_reply(handle, seq, …)` replies
to `g_stored_queries[handle][seq]` and frees the slot; `undeclare` drops all held
slots. `send_reply` no longer clears `reply_keyexpr` (constant per server, re-set
each `try_recv_request`), so successive deferred replies reuse it. Header is
cbindgen-regenerated from the `ffi.rs` stub; `lib.rs` extern + `zpico.rs` wrapper
updated.

**Caveat (pre-existing):** the callback keys the getter by `buffer_index`, which
equals the C queryable handle only because both are allocated without mid-run
reuse (`NEXT_SERVICE_BUFFER_INDEX` is monotonic; `declare_queryable` returns the
first free slot). The existing reply path already relied on this coupling. A fully
robust fix would thread the C handle into the service buffer — tracked separately.

**Original design notes:**
`packages/zpico/zpico-sys/c/zpico/zpico.c` + `packages/zpico/nros-rmw-zenoh/src/shim/service.rs`.
- C shim: `g_stored_queries[ZPICO_MAX_QUERYABLES]` → `[ZPICO_MAX_QUERYABLES][N]` with
  `{ z_owned_query_t; bool in_use }`. `z_query_clone` already yields an owned copy.
- `queryable_handler` (C): allocate a free reply-slot, clone the query in, **pass the
  slot index to the Rust callback** (new callback arg) → Rust writes it into the request
  buffer. Drop the independent `SERVICE_SEQ_COUNTER`.
- `try_recv_request` (Rust): return that slot index as `seq`.
- `zpico_query_reply(handle, seq)`: reply to `g_stored_queries[handle][seq]`, then
  `z_query_drop` it + clear `in_use`. **Owned-query lifetime is the correctness-critical
  bit** — clone on store, drop on reply *and* on queryable teardown, else leak / UAF.

### 237.5 — Tests ◐
**Done:**
- `test_xrce_action_ros2_client` (forward) now hard-asserts the final
  `SUCCEEDED` result — real rclcpp_action get_result deferral over XRCE.
- Backend-agnostic runtime unit tests (`nros-node::executor::tests`):
  - `test_get_result_deferred_per_goal_concurrent` — two concurrently-active goals,
    each with an early get_result (mock seqs 0 + 1), completed **out of order**;
    asserts each reply routes to its OWN correlation token (no cross-wiring) and
    the result bytes match. The exact concurrent case single-slot backends drop.
  - `test_get_result_after_completion_replies_immediately` — get_result after the
    goal terminated → immediate reply from the slab, never deferred (the
    nano-ros↔nano-ros path).
  - `MockServiceServer` now hands out a distinct increasing `sequence_number` per
    request and records `send_reply(seq, data)` so the runtime seq routing is
    asserted without a live transport.
- Regression: the *immediate* reply path stays green on every backend — C
  nano-ros↔nano-ros action + service over Zenoh, XRCE service interop, Cyclone
  `feedback_roundtrip`, and the full `nros-node` lib suite (159 mock / 71 cffi).

**Remaining:**
- Concurrent-goals **e2e** over a live transport (needs a multi-goal action server
  example/bin — the stock example processes one goal at a time, so it can't
  exercise concurrent deferral end-to-end).
- `rmw_zenoh_cpp` ↔ nano-ros Zenoh action interop validation (real ROS 2 over
  Zenoh; needs the rmw_zenoh router + a ROS 2 action client/server).

## Sizing / bounds

`XRCE_MAX_PENDING_REPLIES` (XRCE), per-queryable `N` (Zenoh), `MAX_GOALS` (runtime) all
align to the max concurrent goals — default 4, `#define` / const-overridable. Cyclone
is already 32. Memory: XRCE +`N·24 B`/server, Zenoh +`N·sizeof(z_owned_query)`/queryable
— negligible at N=4.

## Risks

- XRCE/Zenoh `send_reply` semantics change (seq now meaningful). The immediate path is
  unaffected (read `i`, reply `i`); only deferral adds outstanding slots. Guard with the
  existing service round-trip tests.
- Zenoh C↔Rust callback-signature change + owned-query lifetimes is the main surface.
- Back-pressure: a full reply table returns `WOULD_BLOCK`; the runtime must leave the
  request for a later spin rather than drop it (Cyclone already does this).

## Acceptance

- `ros2 action send_goal --feedback` against a nano-ros action server (any backend over
  the agent / DDS) receives accept → feedback → **result**.
- Two concurrent goals, each issuing an early `get_result`, both resolve correctly.
- nano-ros↔nano-ros actions + plain services unchanged.

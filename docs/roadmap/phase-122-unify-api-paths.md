# Phase 122 — Unify C / C++ / Rust API code paths

**Goal:** Eliminate code-path divergence between Rust user-facing API
and the path C/C++ wrappers invoke internally. Reduce test-surface
mismatch.
**Status:** Investigation pass complete. Implementation TBD.
**Priority:** Medium.
**Depends on:** Phase 120 (root cause for the action-server divergence
identified — see `phase-120-baseline-failures.md`).

## Motivation

Phase 120.3 revealed that `Node::create_action_server` (manual-poll)
has a deterministic post-handshake crash on threadx-rv64, while
`Executor::add_action_server` (callback + arena) — the path the
nros-c thin wrapper invokes — does NOT. Switching the Rust example to
the callback model fixed the test.

Naive expectation: "C is a thin wrapper, so identical to Rust." Truth:
C wrapper delegates to a DIFFERENT Rust API entry point than the Rust
example uses. **Same wire-level behavior, different code paths,
different bug surface.**

Unifying the paths means C/C++ tests exercise the same Rust code the
Rust example does. A regression in either path then trips both
language tests — easier to catch.

## Code-path divergences (current state)

| Example | Rust example uses | C wrapper internal path |
|---|---|---|
| **talker** (publisher) | `node.create_publisher` (`session.create_publisher` → returns `EmbeddedPublisher` value) | `session.create_publisher` directly in `nros-c/src/publisher.rs:244` |
| **listener** (subscription) | `node.create_subscription` (manual-poll, returns `Subscription`) | `executor.add_subscription_raw_with_qos_sized` (callback + arena) |
| **service-server** | `executor.add_service` (callback) ✓ | `executor.add_service_raw_sized` (callback) ✓ **already aligned** |
| **service-client** | `node.create_client` (manual-poll, returns `EmbeddedServiceClient`) | `executor.add_service_client_raw_sized` (callback + arena) |
| **action-server** | `executor.add_action_server` (callback) ✓ **fixed in Phase 120.3** | `executor.add_action_server_raw_sized` (callback) ✓ **aligned** |
| **action-client** | `node.create_action_client` (manual-poll, returns `ActionClient`) | `executor.add_action_client_raw` (callback + arena) |
| **timer** | `executor.add_timer` (callback) ✓ | `rust_exec.add_timer` ✓ **aligned** |
| **guard condition** | `executor.add_guard_condition` (callback) ✓ | `rust_exec.add_guard_condition` ✓ **aligned** |

**Diverging entities (manual-poll Rust vs callback C):**

1. **publisher** — Rust uses `node.create_publisher` returning value;
   C uses `session.create_publisher` directly. Different storage
   (Rust returns by value, C stores in `Publisher` struct).
2. **subscription / listener** — Rust manual-poll vs C callback +
   arena.
3. **service-client** — Rust manual-poll vs C callback + arena.
4. **action-client** — Rust manual-poll vs C callback + arena.

**Asymmetry in Rust examples themselves:**

Across `examples/{qemu-riscv64-threadx,qemu-arm-freertos,qemu-arm-nuttx,
threadx-linux,zephyr}/rust/zenoh/`, examples DON'T agree internally:

- Zephyr `listener` uses `executor.add_subscription` (callback).
- All other listener examples use `node.create_subscription` (manual-poll).
- Zephyr `talker` uses `executor.add_timer` (callback-driven).
- All other talker examples use `node.create_publisher` directly.
- Service-server examples uniformly use `executor.add_service` (callback).
- Action-server post-Phase-120.3-fix on rv64 uses
  `executor.add_action_server` (callback); other platforms still on
  `node.create_action_server` (manual-poll).

## Proposal

**Standardize on callback + arena model in ALL Rust examples,
matching what nros-c invokes internally.**

Rationale:

1. **Matches C/C++ wrapper path** — same Rust code under test for both
   language ABIs.
2. **More robust on resource-constrained targets** — the rv64 crash
   shows the manual-poll path can hit issues the callback path doesn't.
   Whether or not we ever fully fix the manual-poll path, callback
   model is the safer default.
3. **Better fits ROS 2 ergonomics** — rclrs 0.7.0 and rclcpp use
   callback dispatch. Manual-poll is a niche API for users who
   explicitly want it.

Keep `Node::create_*` (manual-poll) APIs in the library — they're
useful and other crates may depend on them — but examples + tests
default to the callback model.

## Work items (2026-05-13 design pass)

After the initial investigation, the design pass added new
constraints that grew the phase scope:

- **Two-layer API rule.** L1 (primitive, caller polls) + L2
  (callback, executor-managed) with verb discipline:
  `Node::create_*` (L1) + `Executor::register_*` (L2).
- **Cross-language consistency.** The same two-layer surface in
  Rust, C, and C++.
- **Thin-wrapper discipline.** nros-c / nros-cpp delegate to Rust;
  no duplicated bookkeeping. Detailed in
  `docs/design/nros-c-thin-wrapper-discipline.md`.

Sub-items:

- [x] **122.1 — Rust `add_*` -> `register_*` rename.** Landed
  2026-05-13 (commit `c3c56cc7`). 123 call sites across 37 files.
- [x] **122.2 — C/C++ `nros_executor_add_*` -> `register_*` rename.**
  Landed 2026-05-13 (commit `68e9eef5`). 53 files.
- [x] **122.3.0 — fixture + arena follow-up (post-122.1/2).**
  Two regressions surfaced once `just test-all` ran after the 122.1/2
  renames:
  1. `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` (and
     `tests/size_probe_verify.sh`) still passed the pre-115.M.4 feature
     name `rmw-zenoh` to `cargo build -p nros-c`. Phase 115.M.4 had
     renamed it to `cffi-zenoh-cffi`. 27 C-build tests failed with
     `error: the package 'nros-c' does not contain this feature:
     rmw-zenoh`. Fixed by switching both call sites to
     `cffi-zenoh-cffi,platform-posix,ros-humble`.
  2. `packages/core/nros-node/build.rs` derived `ARENA_SIZE` from
     `MAX_CBS × (rx_buf × 3 + 512)` which budgeted for a triple-
     buffered subscription but not for an `ActionClientRawArenaEntry`.
     Each entry carries 3 `CffiServiceClient`s (each with a 4096-byte
     `pending_request` blocking-fallback buffer) + 3 × rx_buf + a
     `CffiSubscriber`, totalling ~17.5 KB — over the previous 16 KB
     default arena. `arena_alloc` returned `BufferTooSmall`, so
     `nros_cpp_action_client_create` returned `-100`
     (`TRANSPORT_ERROR`) before any traffic. Bumped the per-entry
     budget to `3 × 4480 + 3 × rx_buf + 1536` (~18 KB at default
     rx_buf=1024) so action-client allocation works at the default
     `NROS_EXECUTOR_MAX_CBS=4`. Recovered `test_cpp_action_goal_rejection`.
- [~] **122.3.a — nros-c thin-wrapper audit + discipline doc.**
  In flight 2026-05-13. Output: `docs/design/nros-c-thin-wrapper-discipline.md`.
  Audit result: 4 of 11 entities follow opaque-thin pattern today;
  7 are field-mirror (subscription / service / client / action_server
  / action_client / timer / guard_condition partial).
- [ ] **122.3.b — subscription thin-wrapper refactor + L1 entry
  points.** Template for the remaining four executor-registered
  entities. Adds `nros_subscription_init_polling` (L1 init)
  + `nros_subscription_try_recv_raw` (L1 op). Refactors L2 init
  + register to match the same opaque shape.
- [~] **122.3.c — service / service_client / action_server /
  action_client thin-wrapper refactor + L1 entry points.** Per
  the pattern in 122.3.b. Per-entity L1 ops listed in the
  discipline doc.
  - [x] **122.3.c.1 — Rust `RawServiceServer<REQ, RESP>` +
    `RawServiceClient<REQ, REPLY>` in `nros-node`.** New types
    parallel to `RawSubscription<RX_BUF>`. Methods:
    `try_recv_request_raw` / `send_reply_raw` /
    `send_request_raw` / `try_recv_reply_raw`. `Node::create_service_raw`
    + `create_client_raw` build them. Re-exported from
    `nros_node` crate root.
  - [x] **122.3.c.2 — `SERVICE_SERVER_OPAQUE_U64S` +
    `SERVICE_CLIENT_OPAQUE_U64S` in `nros-c::opaque_sizes`.**
    `u64s_for::<RawServiceServer<MESSAGE_BUFFER_SIZE, …>>()`
    parallels existing `SUBSCRIPTION_OPAQUE_U64S`. Placeholder
    `= 1` when `rmw-cffi` is off.
  - [x] **122.3.c.3 — fix opaque-size emission in the cbindgen
    output.** cbindgen rendered `SUBSCRIPTION_OPAQUE_U64S` and the
    new `SERVICE_SERVER_OPAQUE_U64S` / `SERVICE_CLIENT_OPAQUE_U64S`
    as `#define … 1` in `nros_generated.h` (it picked the
    `#[cfg(not(feature = "rmw-cffi"))]` placeholder branch
    because cbindgen evaluates cfgs against its own command-
    line feature set). The variant header
    (`target/nros-c-generated/nros/nros_config_generated.h`)
    didn't redefine these macros — C consumers saw
    `_opaque[1]` (8 bytes) while Rust wrote a much larger
    `RawSubscription` / `RawServiceServer` value into the same
    storage. Silent struct corruption. Retroactive blocker on
    122.3.b. Fix landed:
    - `nros::sizes` exports `RAW_SUBSCRIPTION_SIZE`,
      `RAW_SERVICE_SERVER_SIZE`, `RAW_SERVICE_CLIENT_SIZE`
      probes via `export_size!` on
      `nros_node::Raw{Subscription,ServiceServer,ServiceClient}`
      with default const generics (= `DEFAULT_RX_BUF_SIZE` =
      `MESSAGE_BUFFER_SIZE`).
    - `nros-c/build.rs` reads the new probes and emits
      `#undef` + `#define` overrides for
      `SUBSCRIPTION_OPAQUE_U64S` /
      `SERVICE_SERVER_OPAQUE_U64S` /
      `SERVICE_CLIENT_OPAQUE_U64S` in the per-build variant
      header. The variant header is included after
      `nros_generated.h`, so the override wins. Current probe
      values at default config: subscription = 205 u64
      (~1640 B), service server = 194 u64 (~1552 B), service
      client = 707 u64 (~5656 B — `pending_request` 4096 +
      buffers).
  - [x] **122.3.c.4 — `nros_service_init_polling`,
    `nros_service_try_recv_request_raw`,
    `nros_service_send_reply_raw` C entry points.** Mirror of
    the subscription template at
    `packages/core/nros-c/src/subscription.rs:269-460`.
    `nros_service_t` gains a `_opaque` field sized at
    `SERVICE_SERVER_OPAQUE_U64S` and a new
    `NROS_SERVICE_STATE_POLLING = 3` variant. `nros_service_fini`
    branches on state — L2 only resets metadata, L1 also
    `drop_in_place`s the inline `RawServiceServer`.
  - [x] **122.3.c.5 — `nros_client_init_polling`,
    `nros_client_send_request_raw`,
    `nros_client_try_recv_reply_raw` C entry points.** Same
    shape as .c.4. `nros_client_t._opaque` sized at
    `SERVICE_CLIENT_OPAQUE_U64S`; new
    `NROS_CLIENT_STATE_POLLING = 4` variant; `nros_client_fini`
    drops the inline `RawServiceClient` on POLLING state.
  - [~] **122.3.c.6 — action L1 polling.** Larger than the service
    pair (5-channel protocol, active-goal tracking).
    `ActionServerCore` / `ActionClientCore` in `nros-node`
    already expose the raw methods, so 122.3.c.6 splits into
    Rust scaffolding (.6.a) and the C entry points (.6.b).
    - [x] **122.3.c.6.a — Rust scaffolding.** Landed:
      `Node::create_action_server_raw[_sized]` /
      `create_action_client_raw[_sized]` in
      `nros-node/src/executor/node.rs` (typeless, take
      `action_name + type_name + type_hash` strings, build the
      5 channels, return `ActionServerCore` / `ActionClientCore`
      directly). New `RAW_ACTION_SERVER_SIZE` /
      `RAW_ACTION_CLIENT_SIZE` probes in `nros::sizes` using
      default const generics
      (`DEFAULT_RX_BUF_SIZE` + `MAX_GOALS = 4`). New
      `ACTION_SERVER_OPAQUE_U64S` /
      `ACTION_CLIENT_OPAQUE_U64S` consts in
      `nros-c::opaque_sizes`. `nros-c/build.rs` emits the
      corresponding `#undef` + `#define` overrides in the
      variant header. Current values at default config:
      action server = 786 u64 (~6.3 KB), action client =
      2193 u64 (~17.5 KB — dominated by 3 × CffiServiceClient
      `pending_request[4096]`).
    - [x] **122.3.c.6.b — C entry points.** Landed:
      `_opaque` field + `STATE_POLLING` variant on both
      `nros_action_server_t` / `nros_action_client_t`. Action
      server L1 surface:
      `nros_action_server_init_polling`,
      `_try_recv_goal_request_raw` (writes goal_id +
      sequence_number out),
      `_accept_goal_raw` / `_reject_goal_raw`,
      `_publish_feedback_raw`, `_complete_goal_raw`,
      `_try_handle_get_result_raw` (takes
      `default_result_cdr`), `_active_goal_count_raw`.
      Action client L1 surface:
      `nros_action_client_init_polling`,
      `_send_goal_raw` (returns generated UUID),
      `_try_recv_goal_response_raw`,
      `_send_get_result_request_raw`, `_try_recv_result_raw`,
      `_send_cancel_request_raw`,
      `_try_recv_feedback_raw` (writes goal_id out). `*_fini`
      branches on state, drops the inline `ActionServerCore`
      / `ActionClientCore` on POLLING. Bonus: new
      `nros_node::ActionServerCore::from_channels`
      constructor exposes the (otherwise crate-private)
      ServerCore fields to the C shim. Server-side
      cancel-handler entry point is split off into .c.6.d
      below — `ActionServerCore::try_handle_cancel` takes a
      per-call cancel-decision closure that doesn't cross the
      C FFI cleanly, so it's exposed via a separate
      peek-then-reply pair.
  - [x] **122.3.c.6.d — server-side cancel-request peek +
    reply (split pair).** Landed: closure-free C-FFI path for
    handling cancel-goal requests. Design picked Option 2
    (peek-then-reply) from the .c.6.d discussion to keep
    closures off the C ABI. Three layers wired together:
    1. `nros-node::ActionServerCore`:
       - new struct `PendingCancelRequest { goal_id,
         sequence_number, current_status }`;
       - new `try_recv_cancel_request()` — non-blocking
         peek, returns `Option<PendingCancelRequest>`;
       - new `send_cancel_reply(sequence_number, return_code,
         &[GoalId])` — builds the action_msgs CDR reply
         (`return_code` + `sequence<GoalInfo>`), flips the
         listed goals to `Canceling`, publishes the status
         array. Existing closure-style `try_handle_cancel`
         left untouched (the Rust-side callback path stays
         supported).
    2. `nros-c::action::server`:
       - `nros_action_server_try_recv_cancel_request_raw(server,
         goal_id_out, sequence_number_out, current_status_out)`;
       - new POD-style enum
         `nros_cancel_return_code_t` (OK / REJECTED /
         UNKNOWN_GOAL / GOAL_TERMINATED — mirrors
         `nros_core::CancelResponse`; named distinctly from
         the pre-existing per-goal `nros_cancel_response_t`
         ACCEPT/REJECT used by the L2 callback path);
       - `_send_cancel_reply_raw(server, sequence_number,
         return_code, accepted, accepted_count)` — accepts a
         contiguous `[u8; 16]` array of goal IDs (cap 8).
    3. `nros-cpp::src::action`:
       - matching `nros_cpp_action_server_try_recv_cancel_request_raw`
         + `_send_cancel_reply_raw` FFI (uses raw `int8_t`
         return_code, defers the enum sugar to the C++ class
         layer);
       - `PollingActionServer<A>::try_recv_cancel_request(goal_id,
         &seq, &current_status)` and
         `::send_cancel_reply(seq, return_code, accepted,
         accepted_count)` methods.
  - [x] **122.3.c.6.e — event-driven path (waker / wake
    callback).** Polling is fine for tight L1 loops; RTOS
    and embassy-style callers want the kernel to wake them
    when data lands. The C ABI surface mirrors the existing
    subscriber / service-client wake plumbing.
    1. **nros-rmw trait.**
       `ServiceServerTrait::register_waker(&Waker)` —
       default no-op; non-supporting backends keep
       compiling. Mirrors the existing methods on
       `SubscriberTrait` / `ServiceClientTrait`.
    2. **zenoh-pico backend.**
       `ServiceBuffer` grew a per-buffer `AtomicWaker`;
       `queryable_callback` calls `buffer.waker.wake()`
       after flipping `has_request`.
       `ZenohServiceServer::register_waker` forwards.
    3. **nros-node convenience.** Methods on the raw
       handles route to the underlying trait method:
       `RawSubscription::register_waker`,
       `RawServiceServer::register_waker`,
       `RawServiceClient::register_waker`,
       `ActionServerCore::register_{goal,cancel,get_result}_waker`,
       `ActionClientCore::register_{goal_response,cancel_response,result,feedback}_waker`.
    4. **C-ABI Waker bridge.** New module
       `nros_node::c_waker` exposes a `CWakeState { fn_ptr,
       ctx }` POD struct and `make_waker(*CWakeState)`. The
       returned `Waker` calls `fn_ptr(ctx)` on wake.
       Caller owns the state's stable address (lifetime
       contract documented).
    5. **C FFI surface.** `nros_wake_state_t` POD struct
       (`[u64; 2]`) lives in nros-c. New entry points:
       - `nros_subscription_set_wake_callback(sub, state,
         cb, ctx)`;
       - `nros_service_set_wake_callback(srv, state, cb,
         ctx)`;
       - `nros_client_set_wake_callback(cli, state, cb,
         ctx)`;
       - `nros_action_server_set_{goal,cancel,get_result}_wake_callback`;
       - `nros_action_client_set_{goal_response,cancel_response,result,feedback}_wake_callback`.
       Caller declares one `nros_wake_state_t` per
       (entity, channel) pair next to the entity and
       passes it in.
    6. **nros-cpp FFI + class methods.** Mirror set:
       `nros_cpp_wake_state_t` + per-entity / per-channel
       `nros_cpp_*_set_*_wake_callback`. The C++ class
       templates `PollingActionServer<A>` /
       `PollingActionClient<A>` expose a nested
       `WakeState` POD and typed
       `set_{goal,cancel,get_result,goal_response,
       cancel_response,result,feedback}_wake_callback`
       methods, so C++ users get the wake hook through the
       same surface they use for try_recv_*.
    Other backends (cyclonedds, dust-dds, XRCE) inherit the
    default no-op `register_waker` for ServiceServer; their
    own subscriber / service-client wake-plumbing already
    works. Future: wire those backends' wake paths through
    once a user surfaces a need.
  - [x] **122.3.c.6.c — cancel-RPC reply receive (client
    side).** Landed: `ActionClientCore::try_recv_cancel_reply`
    in `nros-node/src/executor/action_core.rs`. Symmetric with
    the existing `try_recv_get_result_reply` — drains the
    `cancel_goal_client`'s reply channel into the inline
    `result_buffer`. C FFI: `nros_action_client_try_recv_cancel_response_raw`.
    nros-cpp FFI:
    `nros_cpp_action_client_try_recv_cancel_response_raw`
    (consumed by `PollingActionClient<A>::try_recv_cancel_response`
    in .d.b).
- [~] **122.3.d — nros-cpp wrapper sync.** Mirror the refactored
  C struct shape into the C++ headers. Add L1 constructor +
  `try_recv` method per entity.
  - **Audit (2026-05-13).** nros-cpp's
    `nros_cpp_subscription_*` / `_service_server_*` /
    `_service_client_*` FFI was already L1-polling (stores
    bare `RmwSubscriber` / `RmwServiceServer` /
    `RmwServiceClient` inline, caller drives via
    `try_recv_raw` / `send_reply_raw` / `send_request` /
    `try_recv_reply`). Only the action server / client were
    L2-only (executor arena + callback). 122.3.d narrows to
    actions.
  - [x] **122.3.d.a — Rust FFI for action L1 polling.** New
    functions in `nros-cpp/src/action.rs`:
    `nros_cpp_action_server_init_polling`,
    `_try_recv_goal_request_raw`,
    `_accept_goal_raw` / `_reject_goal_raw`,
    `_publish_feedback_raw`,
    `_complete_goal_raw` (status code = 4/5/6 for
    Succeeded / Canceled / Aborted),
    `_try_handle_get_result_raw`,
    `_destroy_polling`. Action client mirror:
    `_init_polling`, `_send_goal_raw`,
    `_try_recv_goal_response_raw`,
    `_send_get_result_request_raw`,
    `_try_recv_result_raw`, `_send_cancel_request_raw`,
    `_try_recv_feedback_raw`, `_destroy_polling`. Storage
    sized via new
    `NROS_CPP_RAW_ACTION_{SERVER,CLIENT}_OPAQUE_U64S` macros
    emitted in the per-build nros-cpp variant header (build.rs
    extension reads the same `RAW_ACTION_*_SIZE` probes nros-c
    consumes). Companion macros for subscription / service
    server / service client are emitted too in case future
    nros-cpp class polling fields land.
  - [x] **122.3.d.b — C++ class wrappers.** New templates
    `nros::PollingActionServer<A>` (in
    `include/nros/polling_action_server.hpp`) and
    `nros::PollingActionClient<A>` (in
    `include/nros/polling_action_client.hpp`) expose the .d.a
    FFI through a typed C++14 API. Storage is an inline
    `uint64_t storage_[NROS_CPP_RAW_ACTION_{SERVER,CLIENT}_OPAQUE_U64S]`
    field; destructor calls `_destroy_polling`. Server methods:
    `try_recv_goal_request` (deserializes goal),
    `accept_goal` / `reject_goal`, `publish_feedback`,
    `complete_goal`, `try_handle_get_result(default_result)`.
    Client methods: `send_goal`, `try_recv_goal_response` (raw
    bytes — wire-CDR layout doc'd inline),
    `send_get_result_request`, `try_recv_result`
    (deserializes typed result, strips the 5-byte CDR header +
    status-byte prefix), `send_cancel_request`,
    `try_recv_cancel_response`, `try_recv_feedback`. Node gets
    two new method declarations + out-of-line template defns:
    `create_polling_action_server` / `create_polling_action_client`.
    `nros/nros.hpp` pulls in both new headers, so existing
    `#include "nros/nros.hpp"` users get the new templates
    automatically.
- [ ] **122.4 — Rust example migration L1 -> L2 (callback).**
  Mechanical rewrite of non-RTIC example main.rs files. Migration
  list below. RTIC examples stay on L1.
- [ ] **122.5 — docs.** Update book pages, porting guide, and the
  RT positioning page with the unified two-layer story.

## Migration list

Convert these Rust example files to `executor.add_*`:

- `examples/{qemu-riscv64-threadx,qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/rust/zenoh/listener/src/main.rs` → `executor.add_subscription`
- `examples/{*}/rust/zenoh/talker/src/main.rs` — keep publisher but drive via `executor.add_timer` for periodic publish (like Zephyr already does)
- `examples/{*}/rust/zenoh/service-client/src/main.rs` → `executor.add_service_client` (verify API exists; may need same callback-model shape as action-client)
- `examples/{*}/rust/zenoh/action-client/src/main.rs` → `executor.add_action_client`
- Same set for `examples/{*}/rust/xrce/...` and `examples/{*}/rust/dds/...`

## Verification

After migration, every threadx-rv64 / threadx-linux / freertos / nuttx
/ zephyr Rust example exercises the same internal Rust path as the
matching C example. A regression in either path fails both test
families. Phase-120-style "Rust-only" bugs become much harder to
introduce silently.

## Open question — the manual-poll bug itself

This phase **does not fix** the underlying `Node::create_action_server`
crash on rv64. It only routes around it. The crash signature
(deterministic JALR to `0x80251630` = `nx_bsd_socket_pool_memory + 8`)
is documented in `phase-120-baseline-failures.md`. Anyone who needs
the manual-poll API on rv64 will still hit it.

Tracking as a separate open item (not part of this phase): find the
actual STORE that writes `0x80251630` into a function-pointer field
during the manual-poll spin-loop pattern. Watchpoint or
print-on-callback-registration around `zpico_declare_queryable` /
`zpico_declare_subscriber` is the natural next step.

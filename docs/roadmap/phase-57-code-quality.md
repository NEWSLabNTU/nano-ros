# Phase 57 — Code Quality

## Context

An audit of naming conventions, file sizes, unsafe patterns, and feature gating
found several improvement opportunities. None are bugs — all are quality-of-life
improvements that reduce cognitive load, improve grep-ability, and make the
codebase more welcoming.

Principle: **naming clarity > historical inertia; safe wrappers > repeated unsafe
blocks; smaller files > monoliths.**

## Progress

| Item                                    | Status      |
|-----------------------------------------|-------------|
| 57.1 — Remove "Shim" smurf prefix       | Done        |
| 57.2 — Split shim.rs (3,426 lines)      | Done        |
| 57.3 — Split other large files          | Done        |
| 57.4 — Safe buffer accessor wrappers    | Done        |
| 57.5 — Minor unsafe & API cleanups      | Done        |
| 57.6 — TCP/UDP staging deduplication    | Done        |
| 57.7 — nros-c validation macros         | Done        |
| 57.8 — Extract magic constants          | Done        |

## Deliverables

### 57.1 — Remove "Shim" Smurf Prefix

Every type in `shim.rs` and `zpico.rs` carries a `Shim` prefix that adds no
information — the module name already communicates the layer. Users see
`ShimSession` in error messages when `zenoh::Session` or `ZenohSession` would
be clearer.

#### Current state

**`zpico.rs` (low-level C FFI wrappers):**
- `ShimError`, `ShimZenohId`, `ShimLivelinessToken`, `ShimQueryable`,
  `ShimContext`, `ShimPublisher<'a>`, `ShimSubscriber<'a>`

**`shim.rs` (transport trait implementations):**
- `ShimTransport`, `ShimSession`, `ShimPublisher`, `ShimSubscriber`,
  `ShimZeroCopySubscriber`, `ShimServiceServer`, `ShimServiceClient`

**`zpico-sys/src/ffi.rs` (callback type aliases):**
- `ShimCallback`, `ShimCallbackWithAttachment`, `ShimNotifyCallback`,
  `ShimZeroCopyCallback`, `ShimQueryCallback`

**`nros-rmw-zenoh/src/lib.rs` (re-exports as `Zenoh*`):**
- `type ZenohTransport = ShimTransport`, `type ZenohSession = ShimSession`, etc.

**`nros/src/lib.rs` (re-exports as `Rmw*`):**
- `type RmwSession = nros_rmw_zenoh::ShimSession`, etc.

#### Changes

**Phase A — Rename in `zpico.rs`:**

- [x] `ShimError` → `ZpicoError`
- [x] `ShimZenohId` → `ZenohId`
- [x] `ShimLivelinessToken` → `LivelinessToken` (already aliased to this)
- [x] `ShimQueryable` → `Queryable`
- [x] `ShimContext` → `Context`
- [x] `ShimPublisher<'a>` → `Publisher<'a>`
- [x] `ShimSubscriber<'a>` → `Subscriber<'a>`

**Phase B — Rename in `shim.rs`:**

These implement the `nros-rmw` transport traits. Rename to match the public
`Zenoh*` aliases directly, eliminating the alias layer:

- [x] `ShimTransport` → `ZenohTransport`
- [x] `ShimSession` → `ZenohSession`
- [x] `ShimPublisher` → `ZenohPublisher`
- [x] `ShimSubscriber` → `ZenohSubscriber`
- [x] `ShimZeroCopySubscriber` → `ZenohZeroCopySubscriber`
- [x] `ShimServiceServer` → `ZenohServiceServer`
- [x] `ShimServiceClient` → `ZenohServiceClient`

**Phase C — Rename in `zpico-sys/src/ffi.rs`:**

- [x] `ShimCallback` → `ZpicoCallback`
- [x] `ShimCallbackWithAttachment` → `ZpicoCallbackWithAttachment`
- [x] `ShimNotifyCallback` → `ZpicoNotifyCallback`
- [x] `ShimZeroCopyCallback` → `ZpicoZeroCopyCallback`
- [x] `ShimQueryCallback` → `ZpicoQueryCallback`

**Phase D — Update re-exports:**

- [x] `nros-rmw-zenoh/src/lib.rs` — remove `type Zenoh* = Shim*` aliases
      (types are now named `Zenoh*` directly)
- [x] `nros/src/lib.rs` — update `use` paths (no more `Shim*` imports)
- [x] `nros-node/src/executor/spin.rs` — update direct `ShimSession` references
- [x] Update any remaining `Shim*` references across codebase

#### Verification

- [x] `cargo build --workspace` compiles
- [x] `just quality` passes (20 C API/XRCE test failures are pre-existing, unrelated)
- [x] `grep -r 'Shim' packages/` returns zero hits (excluding build.rs internals)

---

### 57.2 — Split `shim.rs` (3,426 lines)

The largest file in the codebase. Contains subscriber buffers, publisher
implementation, service server/client, transport setup, session management,
and zero-copy subscriber — all in one file.

#### Target structure

```
nros-rmw-zenoh/src/
├── shim/
│   ├── mod.rs          — re-exports, shared types (MessageInfo, RmwAttachment,
│   │                     Ros2Liveliness, constants, static counters)
│   ├── transport.rs    — ZenohTransport, ZenohRmw
│   ├── session.rs      — ZenohSession (connect, declare_*, resource management)
│   ├── publisher.rs    — ZenohPublisher, subscriber_callback_with_attachment
│   ├── subscriber.rs   — ZenohSubscriber, ZenohZeroCopySubscriber,
│   │                     SubscriberBuffer, SUBSCRIBER_BUFFERS statics
│   ├── service.rs      — ZenohServiceServer, ZenohServiceClient,
│   │                     ServiceBuffer, SERVICE_BUFFERS statics,
│   │                     queryable_callback
│   └── buffers.rs      — (optional) shared SubscriberBuffer + ServiceBuffer
│                         if buffer types are used across subscriber/service
└── shim.rs             — deleted (replaced by shim/ directory)
```

#### Steps

- [x] Create `shim/` directory
- [x] Move types into sub-modules (one `git mv` + split per module)
- [x] Ensure `pub use` re-exports maintain the same public API
- [x] Update all internal `use` paths within `nros-rmw-zenoh`
- [x] Update external imports (`nros`, `nros-node`, etc.)

#### Verification

- [x] No public API changes (same types accessible from same paths)
- [x] `just quality` passes

---

### 57.3 — Split Other Large Files

| File                              | Lines | Split strategy                                                                      |
|-----------------------------------|-------|-------------------------------------------------------------------------------------|
| `nros-c/src/action.rs`            | 2,074 | Split into `action_server.rs` + `action_client.rs`                                  |
| `rosidl-codegen/src/generator.rs` | 2,075 | Split by output language (C, Rust) or by message type (msg, srv, action)            |
| `nros-c/src/executor.rs`          | 1,528 | Already manageable — skip unless 57.2 pattern proves easy                           |
| `nros-rmw/src/traits.rs`          | 1,343 | Split into `traits/transport.rs`, `traits/session.rs`, `traits/subscriber.rs`, etc. |

#### Steps

- [x] Split `nros-c/src/action.rs` → `action/server.rs` + `action/client.rs`
      + shared `action/common.rs` (UUID, status, type support, callbacks)
- [x] Split `rosidl-codegen/src/generator.rs` → `generator/msg.rs`,
      `generator/srv.rs`, `generator/action.rs`, `generator/common.rs`
- [x] Evaluate `nros-rmw/src/traits.rs` — 1,343 lines, below 1,400 threshold, skip

#### Verification

- [x] `just quality` passes
- [x] No public API changes

---

### 57.4 — Safe Buffer Accessor Wrappers

`shim.rs` has 40+ `unsafe { &SUBSCRIBER_BUFFERS[index] }` and
`unsafe { &mut SERVICE_BUFFERS[index] }` blocks. The safety invariant is
always the same: "we own this buffer index and access is atomic."

#### Changes

- [x] Add `SubscriberBufferRef` wrapper:
      ```rust
      struct SubscriberBufferRef { index: usize }
      impl SubscriberBufferRef {
          fn get(&self) -> &SubscriberBuffer {
              // Safety: index validated at construction time,
              // access is atomic (no data races)
              unsafe { &SUBSCRIBER_BUFFERS[self.index] }
          }
      }
      ```
- [x] Add `ServiceBufferRef` wrapper (same pattern)
- [x] Replace all `unsafe { &SUBSCRIBER_BUFFERS[idx] }` with `self.buf.get()`
- [x] Replace all `unsafe { &mut SERVICE_BUFFERS[idx] }` with `self.buf.get_mut()`
- [x] Validate `index < MAX` at construction time (panic on OOB)

#### Impact

- Eliminates ~40 unsafe blocks (safety proven once at construction)
- Makes buffer access grep-able and auditable
- No runtime cost (optimizer inlines the wrapper)

#### Verification

- [x] `just quality` passes
- [x] Miri tests still pass (no UB introduced)

---

### 57.5 — Minor Unsafe & API Cleanups

Small improvements found during the audit. Each is independent.

#### GuardConditionHandle raw pointer

**File:** `nros-node/src/executor/types.rs`

- [x] Replace `flag: *const AtomicBool` with `flag: &'static AtomicBool`,
      removing the need for `unsafe impl Send/Sync`. The `'static` lifetime
      is asserted at construction time via `unsafe fn new()`.

#### CsMutex: prefer `with()` over `lock()`

**File:** `nros-rmw/src/sync.rs`

- [x] Add `#[deprecated]` on `cs_impl::lock()` pointing to `with()`
- [x] Add `with()` method to `spin_impl::Mutex` for API consistency
- [x] Document that `with()` (closure-based) is the preferred API

#### Document AtomicBool ABI assumption

**File:** `nros-rmw-zenoh/src/shim/subscriber.rs`

- [x] Add comment explaining `AtomicBool::as_ptr() as *const bool` cast
      assumes identical ABI (true for all Rust targets)

#### XRCE init wrapper safety

**File:** `nros/src/lib.rs`

- [x] Mark `init_posix_udp()` and `init_posix_serial()` wrappers as
      `unsafe fn` — inner functions have safety preconditions (no concurrent
      calls, single active transport) that the wrapper cannot verify.

#### Verification

- [x] `just quality` passes

---

### 57.6 — TCP/UDP Staging Deduplication

`bridge.rs` has ~250 lines of near-identical code between TCP and UDP socket
operations. `SocketEntry` and `UdpSocketEntry` share the same staging buffer
fields (`rx_pos`, `rx_len`, `tx_pos`, `tx_len`) and identical recv/send/compact
logic.

#### Duplicated pairs

| TCP function                       | UDP function                       | Similarity                                                  |
|------------------------------------|------------------------------------|-------------------------------------------------------------|
| `register_socket` (259-282)        | `register_udp_socket` (287-309)    | ~90% — same slot search, ephemeral port alloc, buffer reset |
| `socket_recv` (582-610)            | `udp_socket_recv` (737-765)        | ~95% — identical available calc, copy, pos reset            |
| `socket_send` (614-637)            | `udp_socket_send` (769-794)        | ~85% — identical space calc, copy; UDP adds endpoint        |
| `SocketEntry` (32-54)              | `UdpSocketEntry` (109-132)         | ~70% — same staging fields, UDP adds per-packet endpoint    |
| poll TCP TX drain (372-382)        | poll UDP TX drain (434-454)        | ~60% — TCP incremental, UDP atomic                          |
| poll TCP RX compact+fill (387-404) | poll UDP RX compact+fill (459-476) | ~95% — identical compaction and fill                        |

#### Changes

- [x] Extract common staging fields into `StagingState` struct with methods:
      `recv()`, `send()`, `compact_rx()`, `advance_tx()`, `advance_rx()`,
      `reset_tx()`, `reset()`, `has_rx_data()`, `has_tx_space()`,
      `has_tx_pending()`, `tx_pending()`, `rx_space()`
- [x] Embed `StagingState` in both `SocketEntry` and `UdpSocketEntry`
- [x] Refactor `socket_recv`/`udp_socket_recv` to delegate to
      `StagingState::recv()`
- [x] Refactor `socket_send`/`udp_socket_send` to delegate to
      `StagingState::send()`
- [x] Refactor poll compaction/fill to delegate to `StagingState` methods
- [x] Extract `allocate_ephemeral_port()` helper for shared ephemeral port
      logic
- [x] Add `const INIT` to `SocketEntry` and `UdpSocketEntry` (replaces
      verbose static initializers)

#### Impact

- Eliminates ~200 lines of duplication
- Single source of truth for staging buffer invariants
- `StagingBufferGhost` (from Phase 56.3) directly mirrors `StagingState`

#### Verification

- [x] `just quality` passes
- [x] QEMU networked tests pass (TCP + UDP paths exercised)

---

### 57.7 — nros-c Validation Macros

Every C API function in `nros-c` starts with the same null-pointer and state
checks, copy-pasted 50+ times across service.rs, action.rs, publisher.rs,
subscription.rs, executor.rs, and timer.rs.

#### Current pattern (repeated ~50 times)

```rust
if service.is_null() || node.is_null() || type_info.is_null() {
    return NROS_RET_INVALID_ARGUMENT;
}
let service = unsafe { &mut *service };
if service.state != nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED {
    return NROS_RET_NOT_INIT;
}
```

#### Changes

- [x] Create `packages/core/nros-c/src/macros.rs` with `validate_not_null!`
      and `validate_state!` (2-arg returns `NROS_RET_NOT_INIT`, 3-arg returns
      custom error code)
- [x] Replace all null-check boilerplate in service.rs (16 call sites)
- [x] Replace all null-check boilerplate in action/ (19 call sites:
      11 server + 8 client)
- [x] Replace all null-check boilerplate in publisher.rs (7 call sites)
- [x] Replace all null-check boilerplate in subscription.rs (5 call sites)
- [x] Replace all null-check boilerplate in executor.rs (27 call sites:
      13 null + 14 state)
- [x] Replace all null-check boilerplate in timer.rs (7 call sites)
- [x] Replace all null-check boilerplate in guard_condition.rs (10 call sites)

#### Impact

- Eliminates ~100 lines of boilerplate
- Consistent validation across all C API entry points
- Easier to audit: `grep validate_not_null` finds all validation sites

#### Verification

- [x] `just quality` passes
- [ ] `just test-c` passes
- [ ] Kani harnesses for null-pointer checks still pass

---

### 57.8 — Extract Magic Constants

Several raw literals appear in production code where named constants would
improve readability and auditability.

#### ROS 2 liveliness protocol markers

**File:** `nros-rmw-zenoh/src/shim.rs:336-497`

Bare strings like `"0/0/NN"`, `"0/11/MP"`, `"0/11/MS"` encode rmw_zenoh
protocol constants. These should be named:

- [x] `const LIVELINESS_PREFIX: &str = "@ros2_lv"`
- [x] `const ENTITY_NODE: &str = "NN"`
- [x] `const ENTITY_PUBLISHER: &str = "MP"`
- [x] `const ENTITY_SUBSCRIBER: &str = "MS"`
- [x] `const ENTITY_SERVICE_SERVER: &str = "SS"`
- [x] `const ENTITY_SERVICE_CLIENT: &str = "SC"`
- [x] `const PROTO_VERSION_NODE: &str = "0/0"` (node liveliness version)
- [x] `const PROTO_VERSION_TOPIC: &str = "0/11"` (topic/service liveliness version)

#### Repeated polling interval

**File:** `nros-node/src/executor/handles.rs:280, 751, 850`

`spin_interval_ms: 10u64` appears 3 times:

- [x] Extract `const DEFAULT_SPIN_INTERVAL_MS: u64 = 10`
- [x] Replace all 3 call sites

#### GoalId structure sizes

**File:** `nros-node/src/executor/handles.rs:405-407`

Hardcoded `16` for UUID byte count and implicit `4` for length prefix:

- [x] `const GOAL_UUID_SIZE: usize = 16`
- [x] `GOAL_ID_CDR_HEADER` documented in `GOAL_UUID_SIZE` doc comment
      (implicit in `read_u32()` — no literal `4` to replace)
- [x] Replace `for _ in 0..16` (2 sites) with `for _ in 0..GOAL_UUID_SIZE`

#### Verification

- [x] `just quality` passes
- [x] `grep -rn '0/0/NN\|0/11/MP\|0/11/MS\|0/11/SS\|0/11/SC' packages/`
      returns zero hits in non-constant code

## Implementation Order

```
57.1 (rename Shim*) ───→ 57.2 (split shim.rs) ───→ 57.4 (safe wrappers)
                                │                         │
57.3 (split other files) ──────┤── parallel ──────────────┘
57.5 (minor cleanups)   ───────┤
57.6 (TCP/UDP dedup)    ───────┤
57.7 (nros-c macros)    ───────┤
57.8 (magic constants)  ───────┘
```

57.1 before 57.2: rename first so the new module files start with clean names.
57.2 before 57.4: split first so wrapper types land in the right sub-module.
57.6–57.8 are independent of each other and of 57.1–57.5.

## Key Files

| File                                                        | Change                        |
|-------------------------------------------------------------|-------------------------------|
| `packages/zpico/nros-rmw-zenoh/src/zpico.rs`                | Rename `Shim*` types          |
| `packages/zpico/nros-rmw-zenoh/src/shim.rs`                 | Rename + split into `shim/`   |
| `packages/zpico/nros-rmw-zenoh/src/lib.rs`                  | Remove alias layer            |
| `packages/zpico/zpico-sys/src/ffi.rs`                       | Rename callback types         |
| `packages/core/nros/src/lib.rs`                             | Update re-exports             |
| `packages/core/nros-c/src/action.rs`                        | Split server/client           |
| `packages/core/nros-c/src/macros.rs`                        | New: validation macros        |
| `packages/core/nros-c/src/service.rs`                       | Use validation macros         |
| `packages/core/nros-c/src/publisher.rs`                     | Use validation macros         |
| `packages/codegen/packages/rosidl-codegen/src/generator.rs` | Split by type                 |
| `packages/core/nros-node/src/executor/types.rs`             | GuardConditionHandle          |
| `packages/core/nros-node/src/executor/handles.rs`           | Extract magic constants       |
| `packages/core/nros-rmw/src/sync.rs`                        | CsMutex API guidance          |
| `packages/zpico/zpico-smoltcp/src/bridge.rs`                | TCP/UDP staging deduplication |

## Verification

1. `just quality` — no regressions (format, clippy, nextest, miri, QEMU)
2. `grep -r 'Shim' packages/ --include='*.rs'` — zero hits in non-comment code
3. No public API changes (same types accessible from same crate paths)

## Out of Scope

- **no_std refactoring**: Audit found the codebase already has excellent no_std
  discipline. All `std`/`alloc` usage is behind proper feature gates with
  legitimate need (clock, sleep, heap for large responses, env vars). No
  actionable items.
- **nros-c unsafe**: The C FFI crate (`nros-c`) legitimately needs `unsafe`
  for 420+ FFI operations. The `#![allow(unsafe_op_in_unsafe_fn)]` is
  intentional for edition 2024.
- **zpico-sys/xrce-sys unsafe**: FFI binding crates are inherently unsafe.
- **Unwrap/panic cleanup**: 90%+ of `.unwrap()` calls are in `#[cfg(test)]`.
  Only 5 `panic!()` in production code, all guarding impossible states. Zero
  `todo!()` or `unimplemented!()`. No action needed.
- **Dead code/stale comments**: All 14 `#[allow(dead_code)]` annotations are
  justified with comments. No stale references to old crate names. Clean.
- **C example deduplication**: Zenoh and XRCE examples share signal handlers
  and init sequences, but examples should remain self-contained for
  learnability. Not worth extracting a shared template.

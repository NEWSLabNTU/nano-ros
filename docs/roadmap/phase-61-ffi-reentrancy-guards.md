# Phase 61 — FFI Reentrancy Guards

**Goal**: Add `critical_section::with()` guards around all RMW FFI calls to prevent
data corruption from concurrent access, enabling mixed-priority RTIC tasks and safe
multi-task access on RTOS platforms.

**Status**: Complete

**Priority**: Medium

**Depends on**: None (benefits RTIC, FreeRTOS, NuttX, ThreadX)

## Overview

Both RMW backends (zenoh-pico and XRCE-DDS) use global static state with zero
synchronization on bare-metal. This is safe when only one task accesses the transport
at a time (e.g., RTIC with all tasks at priority 1), but corrupt when:

- RTIC tasks run at **mixed priorities** (higher-priority task preempts mid-FFI call)
- RTOS tasks on FreeRTOS/NuttX/ThreadX call FFI from multiple threads without locks
- Interrupt handlers attempt transport operations

The fix is wrapping all FFI calls in `critical_section::with()` at the Rust boundary.
On Cortex-M, this disables interrupts via PRIMASK. On RTOS platforms, the
`critical-section` crate's implementation uses the RTOS's native critical section
(e.g., `taskENTER_CRITICAL()` on FreeRTOS).

## Design

### Reentrancy Analysis — Zenoh-pico (rmw-zenoh)

The zpico C shim (`zpico.c`) uses global static state with **zero synchronization**:
- `g_session`, `g_config`, `g_session_open`
- `g_publishers[]`, `g_subscribers[]`, `g_queryables[]`
- `g_stored_queries[]`, `g_pending_gets[]`, `g_liveliness[]`

Bare-metal platform stubs provide no-op `_z_mutex_lock/unlock()` implementations.
The `sync-critical-section` feature only protects Rust wrapper state (sequence
counters, buffer pointers), NOT the underlying C FFI calls.

18+ public functions access this global state:

| Category       | Functions                                                                        | Global state accessed                                 |
|----------------|----------------------------------------------------------------------------------|-------------------------------------------------------|
| Session        | `init_with_config`, `open`, `close`                                              | `g_config`, `g_session`, `g_session_open`, all arrays |
| Publisher      | `declare_publisher`, `publish`, `publish_with_attachment`, `undeclare_publisher` | `g_publishers[]`, `g_session`                         |
| Subscriber     | `declare_subscriber*` (4 variants), `undeclare_subscriber`                       | `g_subscribers[]`, `g_session`                        |
| Queryable      | `declare_queryable`, `undeclare_queryable`, `query_reply`                        | `g_queryables[]`, `g_stored_queries[]`, `g_session`   |
| Service client | `get`, `get_start`, `get_check`                                                  | `g_pending_gets[]`, `g_session`                       |
| Liveliness     | `declare_liveliness`, `undeclare_liveliness`                                     | `g_liveliness[]`, `g_session`                         |
| I/O            | `spin_once`, `poll`                                                              | `g_session` (+ callbacks run inside)                  |

**Subscriber buffer reads** are the one exception: `try_recv()` uses an atomic `locked`
flag that coordinates with the C callback, so the buffer copy itself is safe under
preemption. But publisher, session, and queryable operations are not.

### Reentrancy Analysis — XRCE-DDS (rmw-xrce)

The XRCE backend (`nros-rmw-xrce/src/lib.rs`) uses global static state at the Rust
level — **more** of it than zpico (~21 KB total):

| Global static              | Type / Size                       | Synchronization |
|----------------------------|-----------------------------------|-----------------|
| `TRANSPORT`                | `uxrCustomTransport`              | None            |
| `SESSION`                  | `uxrSession`                      | None            |
| `OUTPUT_RELIABLE`          | `uxrStreamId`                     | None            |
| `INPUT_RELIABLE`           | `uxrStreamId`                     | None            |
| `PARTICIPANT_ID`           | `uxrObjectId`                     | None            |
| `NEXT_ENTITY_ID`           | `u16`                             | None            |
| `INITIALIZED`              | `bool`                            | None            |
| `OUTPUT_RELIABLE_BUF`      | `[u8; STREAM_BUFFER_SIZE]` (2 KB) | None            |
| `INPUT_RELIABLE_BUF`       | `[u8; STREAM_BUFFER_SIZE]` (2 KB) | None            |
| `SUBSCRIBER_SLOTS`         | `[SubscriberSlot; 8]` (~8 KB)     | Atomics only    |
| `SERVICE_SERVER_SLOTS`     | `[ServiceServerSlot; 4]` (~4 KB)  | Atomics only    |
| `SERVICE_CLIENT_SLOTS`     | `[ServiceClientSlot; 4]` (~4 KB)  | Atomics only    |

The C library's mutex support (`UCLIENT_PROFILE_MULTITHREAD`) is **not defined** in
the build configuration — all `UXR_LOCK/UXR_UNLOCK` macros compile to **no-ops**.

Operations accessing global state:

| Category       | Operations                                                                               | Global state accessed                                       |
|----------------|------------------------------------------------------------------------------------------|-------------------------------------------------------------|
| Session        | `open`, `close`, `init_transport`                                                        | `TRANSPORT`, `SESSION`, stream IDs, `INITIALIZED`, all slots|
| Publisher      | `publish_raw` (+ fragmented path)                                                        | `SESSION`, `OUTPUT_RELIABLE`                                |
| Subscriber     | `declare`, `try_recv_raw`, `process_raw_in_place`                                        | `SESSION`, `SUBSCRIBER_SLOTS[]`                             |
| Service server | `declare`, `try_recv_request`, `send_reply`                                              | `SESSION`, `SERVICE_SERVER_SLOTS[]`                         |
| Service client | `declare`, `send_request_raw`, `try_recv_reply_raw`, `call_raw`                          | `SESSION`, `SERVICE_CLIENT_SLOTS[]`                         |
| Entity mgmt    | `create_datawriter`, `create_datareader`, `create_requester`, `create_replier`           | `SESSION`, `NEXT_ENTITY_ID`, per-type slot arrays           |
| I/O            | `spin_once` (`uxr_run_session_time`)                                                     | `SESSION` (+ callbacks run inside)                          |

**XRCE-specific hazards beyond zpico**:

1. **Torn `SampleIdentity` reads**: `request_callback()` writes
   `slot.sample_id = *sample_id` — a 20-byte struct assignment (`GUID_t` +
   `SequenceNumber_t`) with NO synchronization. A higher-priority task calling
   `try_recv_request()` can see a partially-written `SampleIdentity`, causing reply
   routing failure.

2. **No `locked` flag on service slots**: Unlike `SubscriberSlot` which has a `locked`
   `AtomicBool` to coordinate callback vs. reader, `ServiceServerSlot` and
   `ServiceClientSlot` have **no lock protection at all**. The callback writes directly
   to `slot.data[]` while a reader may be mid-copy.

3. **Non-atomic `active` field**: Callbacks iterate all slots checking `slot.active`
   (a plain `bool`). Creating/destroying entities from a higher-priority task while
   a callback is mid-iteration can cause wrong-slot data delivery.

4. **Entity ID collision**: `NEXT_ENTITY_ID` (a plain `u16`) is incremented during
   entity creation. Concurrent creation from mixed-priority tasks can produce duplicate
   IDs.

**Transport statics** (`xrce-smoltcp`): `TX_STAGING`, `RX_STAGING`, `UDP_HANDLE_RAW`,
etc. are also unprotected globals. These are accessed from XRCE transport callbacks
which run inside `spin_once()` — transitively protected (see below).

### Callbacks Protected Transitively

On bare-metal, callbacks are invoked synchronously inside `spin_once()` for both
backends. For zpico (`Z_FEATURE_MULTI_THREAD=0`): `sample_handler()` and
`query_handler()` run inside `zp_read()` within `zpico_spin_once()`. For XRCE (no
`UCLIENT_PROFILE_MULTITHREAD`): `topic_callback()`, `request_callback()`, and
`reply_callback()` run inside `uxr_run_session_time()`. Wrapping `spin_once()` in a
critical section protects all callbacks transitively. No separate callback protection
needed.

For XRCE, the torn-`SampleIdentity` and missing-`locked`-flag hazards are automatically
resolved under critical section because callbacks cannot be interrupted mid-write.

### Implementation Pattern

**Non-I/O FFI calls** — wrap directly:

```rust
pub fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
    critical_section::with(|_cs| {
        let ret = unsafe { zpico_publish(self.handle, data.as_ptr(), data.len()) };
        if ret < 0 { Err(TransportError::from(ret)) } else { Ok(()) }
    })
}
```

### spin_once() Timeout Decomposition

Memory safety must not depend on callers using `timeout=0`. Any `spin_once(N)` call
is decomposed into a loop of guarded `spin_once(0)` calls. Each iteration runs inside
its own critical section (bounded, short). Between iterations, interrupts are re-enabled,
allowing preemption. Elapsed time is measured via platform clock:

```rust
pub fn spin_once(&self, timeout_ms: u32) -> Result<i32> {
    // First poll — always non-blocking inside critical section
    let ret = critical_section::with(|_cs| {
        unsafe { zpico_spin_once(0) }
    });
    if ret > 0 || timeout_ms == 0 {
        return handle_ret(ret);
    }

    // Remaining time: loop with short critical sections
    let deadline = z_clock_now() + timeout_ms;
    loop {
        if z_clock_now() >= deadline {
            return Ok(0); // timeout
        }
        let ret = critical_section::with(|_cs| {
            unsafe { zpico_spin_once(0) }
        });
        if ret > 0 {
            return handle_ret(ret);
        }
        // Interrupts re-enabled here — higher-priority tasks can preempt
    }
}
```

```
spin_once(100) with FFI guard:

  CS{ zpico_spin_once(0) }  →  preemptable  →  CS{ zpico_spin_once(0) }  →  ...
  |------ ~µs ------|          |-- safe --|     |------ ~µs ------|

  Each critical section is bounded to one non-blocking FFI call.
  Total elapsed checked via platform clock each iteration.
  Loop exits when data processed OR elapsed >= timeout_ms.
```

This maintains:
- **Memory safety**: every FFI call is inside a critical section, unconditionally
- **Real-time**: critical sections are bounded to one `spin_once(0)` call (~µs)
- **Correctness**: total timeout is honored via the external loop
- **Preemptability**: higher-priority tasks can preempt between iterations

Same pattern applies to XRCE (`uxr_run_session_time(0)` instead of `zpico_spin_once(0)`).

### Interrupt Latency Trade-off

Each guarded `spin_once(0)` call adds a few microseconds of interrupt-disabled time
(buffer write + protocol encode + possibly one packet read with callbacks). Acceptable
for most Cortex-M applications (typical ISR latency budgets are 100µs+). The
`publish()` / `query_reply()` / etc. calls are even shorter.

### Feature Flag Design

Gate behind feature `ffi-sync` (off by default). A single `ffi_guard()` helper
dispatches at compile time — no feature gates needed at call sites:

```rust
#[inline(always)]
fn ffi_guard<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(feature = "ffi-sync")]
    { return critical_section::with(|_cs| f()); }
    #[cfg(not(feature = "ffi-sync"))]
    f()
}

// Call sites are feature-agnostic:
fn publish(&self, data: &[u8]) -> Result<()> {
    let ret = ffi_guard(|| unsafe { zpico_publish(self.handle, data.as_ptr(), data.len()) });
    if ret < 0 { Err(ZpicoError::from_code(ret)) } else { Ok(()) }
}
```

The feature is off by default because:
- Single-task and same-priority configurations don't need it
- It adds interrupt-disabled time on every FFI call
- Users who need it (RTIC mixed-priority, multi-task RTOS) explicitly opt in

## Work Items

- [x] 61.1 — Add critical-section guards to zpico FFI
- [x] 61.2 — Add critical-section guards to XRCE FFI

### 61.1 — Add Critical-Section Guards to zpico FFI

Wrap all 18+ zpico C shim calls in `critical_section::with()` at the Rust FFI boundary.
No C-side changes needed.

**Implementation**:
- Gate behind `ffi-sync` feature
- Wrap each zpico FFI call in `critical_section::with()` in the Rust shim layer
- `spin_once(N)` decomposed into loop of guarded `spin_once(0)` calls

**Status**: Complete

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/zpico.rs` — `ffi_guard()` helper + all Context/Drop method wrappers
- `packages/zpico/nros-rmw-zenoh/src/shim/service.rs` — `try_recv_request()` buffer read guard
- `packages/zpico/nros-rmw-zenoh/Cargo.toml` — new `ffi-sync` feature + `critical-section` dep
- `packages/zpico/zpico-sys/c/zpico/zpico.c` — `zpico_clock_start()` / `zpico_clock_elapsed_ms_since()` helpers
- `packages/zpico/zpico-sys/src/ffi.rs` — clock helper cbindgen stubs
- `packages/zpico/zpico-sys/src/lib.rs` — clock helper extern declarations

### 61.2 — Add Critical-Section Guards to XRCE FFI

Same strategy as 61.1 but for the XRCE-DDS backend. Wrap all global-state-accessing
operations in `critical_section::with()` at the Rust level. Since the XRCE wrapper
accesses globals directly (not via a C shim), all guards are pure Rust changes — no
C-side modifications needed.

**Additional fixes unlocked by critical-section guards**:
- **Torn `SampleIdentity`**: `request_callback()` writes `slot.sample_id` (20 bytes)
  non-atomically. Under critical section, the callback cannot be interrupted mid-write.
- **Missing `locked` flag**: `ServiceServerSlot` and `ServiceClientSlot` have no
  reader/writer coordination. Under critical section, callback and reader cannot overlap.
- **Non-atomic `active` field**: Slot iteration in callbacks reads `slot.active` (plain
  `bool`). Under critical section, entity creation cannot race with callback dispatch.

**Implementation**:
- Gate behind the same `ffi-sync` feature as 61.1
- Wrap each `unsafe` block touching global statics in `critical_section::with()`
- `spin_once(N)` decomposed into loop of guarded `uxr_run_session_time(0)` calls

**Status**: Complete

**Files**:
- `packages/xrce/nros-rmw-xrce/src/lib.rs` — `ffi_guard()` helper + guards on all global-state operations
- `packages/xrce/nros-rmw-xrce/Cargo.toml` — `ffi-sync` feature + `critical-section` dep

## Acceptance Criteria

- [x] All zpico FFI calls wrapped in `critical_section::with()` when feature enabled
- [x] All XRCE global-state operations wrapped when feature enabled
- [x] `spin_once(N)` decomposed into guarded `spin_once(0)` loop for both backends
- [x] Feature is off by default — existing behavior unchanged without flag
- [x] No performance regression when feature disabled (zero-cost abstraction)
- [ ] RTIC examples work with mixed priorities when feature enabled
- [x] `just quality` passes (with and without feature)

## Notes

- **Feature naming**: `ffi-sync` — short, descriptive. Complements `sync-critical-section`
  (which protects Rust wrapper state only). `ffi-sync` extends protection to the FFI boundary
- **RTOS benefit**: Also benefits FreeRTOS, NuttX, and ThreadX platforms where multiple
  tasks may call FFI concurrently. The `critical-section` crate dispatches to
  platform-appropriate implementations (PRIMASK on bare-metal, `taskENTER_CRITICAL()`
  on FreeRTOS, `irq_lock()` on Zephyr)
- **Not a replacement for proper multi-threading**: For high-throughput multi-core
  scenarios, finer-grained locking or per-core sessions would be better. This feature
  targets single-core MCUs with interrupt-level concurrency
- **POSIX note**: On POSIX, zenoh-pico already has internal mutex-based multi-threading
  (`Z_FEATURE_MULTI_THREAD=1`), so enabling this feature adds redundant (but harmless)
  synchronization. Not recommended for POSIX — only useful on bare-metal and RTOS
  platforms where the C library has no-op mutex stubs

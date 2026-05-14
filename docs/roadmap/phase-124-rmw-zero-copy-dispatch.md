# Phase 124 — RMW zero-copy + wake-callback dispatch + ABI extensions

**Goal.** Close the cffi RMW coverage gaps identified by the
2026-05-14 `docs/research/rmw-c-abi-coverage.md` analysis without
inheriting upstream `rmw.h`'s waitset-shaped, dishonest-stub model.
Six coordinated additions to `nros-rmw-cffi`:

1. **Unified zero-copy ABI** — one set of vtable slots used by
   both Rust (`SlotLending`/`SlotBorrowing`) and C/C++ callers.
   Replaces Phase 99's never-shipped slots + the per-publisher
   `TxArena` fallback in `nros-node` with a single canonical
   path. Closes the "Rust gets zero copy, C/C++ doesn't" gap.
2. **Wake-callback + condvar layer** — builds incrementally on
   phase 104.C.6.b's flag-based wake (commit `4c5cb87f`).
   Vtable slot `set_wake_signal(*flag)` evolves into
   `set_wake_callback(cb, ctx)`; backend calls `cb(ctx)` on
   async wake; runtime-supplied `cb` writes the existing
   `Executor.wake_flag` AND signals a new `Executor.wake_cv`
   atomically. Spin loop blocks on the condvar (sub-poll wake
   latency); falls back to C.6.b's flag-only behaviour on
   backends that leave the callback slot NULL. Replaces
   upstream `rmw_wait` + `rmw_guard_condition_t` with platform-
   condvar dispatch (no waitset → no RTOS stubs). One-line
   backend obligation; ISR-safe contract.
3. **Service availability probe** — `service_server_available()`.
   Closes the "client startup ordering" gap.
4. **Sequence take** — `try_recv_sequence(buf, per_msg_cap, max,
   out_lens)`. Burst-receive without N×vtable-dispatch overhead.
5. **Continuous serialization** — `publish_streamed(size_cb,
   ser_cb, user_ctx)`. Stream into transport buffer in chunks;
   avoids a staging buffer for big messages. Lesson from
   micro-ROS's `rmw_uros_set_continous_serialization_callbacks`.
6. **Ping primitive** — `ping_session(timeout_ms)`. Light-weight
   "is the peer/agent up?" probe. Lesson from micro-ROS's
   `rmw_uros_ping_agent`.

**Status.** Plan opened 2026-05-14 after the RMW coverage audit.
None of the six items implemented yet. Spec'd here; sub-phase
order in §"Work items" below.

**Priority.** P1 (zero-copy + dispatch) / P2 (probe + sequence +
continuous + ping). P1 items unblock the Phase 110 PiCAS work
(dispatch wake-latency), the cross-language zero-copy story
(Phase 122 + 123 promise), and remove the largest "Rust path
≠ C path" discrepancy.

**Depends on.** Phase 104.B (named registry; vtable shape frozen).
Phase 121 (canonical platform-cffi — `nros_platform_condvar_*`
already there). Phase 122 (cbindgen canonical FFI). Phase 123.A.11
(per-target nros-c — vtable additions don't trigger per-RMW
rebuilds).

**Cross-language discipline.** Per Phase 122, every new vtable
slot gets a matching Rust trait method + thin C/C++ wrapper. The
cffi vtable is the source of truth; Rust `SlotLending` /
`SlotBorrowing` become wrappers over the vtable, not parallel
implementations.

## Background

### Today's surface coverage gaps

From `docs/research/rmw-c-abi-coverage.md`:

| Feature | Today | Gap |
|---|---|---|
| Zero copy via cffi | ❌ | C/C++ users have no path; Rust `SlotLending` works only on zenoh-pico + `rmw-lending` feature; arena fallback is single-copy. |
| Wait set / wake latency | 🔀 poll model | `drive_io(timeout)` is bounded by the timeout, not by data arrival. Phase 110 PiCAS needs sub-poll-period wake. |
| Guard condition | 🔀 Rust-only | `nros_executor_stop()` sets a flag; next `drive_io` iteration sees it. No way to wake from ISR / signal handler in C. |
| Service availability probe | ❌ | Client startup must time-out a `call_raw` and infer. |
| Sequence take | ❌ | Burst sensors loop `try_recv_raw`; N × vtable dispatch. |
| Continuous serialization | ❌ | `publish_raw(bytes, len)` requires pre-encoded payload — wasteful for big msgs on small RAM. |
| Ping | ❌ | "Is the agent up?" requires a full `Executor::open` to find out. |

### Won't-do (deliberate)

Reaffirmed from the coverage audit:

- **Graph introspection** (`get_node_names`, `count_publishers`,
  etc.). Embedded targets know topology at deploy time.
- **GIDs**. Rust monomorphisation + named registry cover identity.
- **Allocation hooks** (`rcutils_allocator_t`). Arena model.
- **Content filter**. Heavy backend feature; rare in embedded.
- **Discovery** (UDP/TCP autoscan). Explicit-locator is the
  embedded-friendly choice.

### Why not import upstream `rmw_wait`?

Upstream defines `rmw_wait(waitset, timeout)` as the dispatch
primitive in the RMW layer. Two problems for nano-ros:

1. **Backend obligation creep.** Every backend must implement
   `rmw_wait`. ThreadX has no native waitset; bare-metal has
   none either. micro-ROS's solution is to stub 14 .c files with
   `RMW_RET_UNSUPPORTED` — dishonest about what works.
2. **Platform-shaped assumption.** Upstream waitset assumes
   POSIX-style `select/epoll/k_poll` semantics. Doesn't fit
   bare-metal Cortex-M event loops or ThreadX event-flag groups
   without backend-specific scaffolding.

Phase 124 chooses to keep wait semantics in the **platform
layer** (where condvar / event-flags / WFI ARE the
abstractions) and let the RMW backend reduce to a one-line
"call `(wake_cb)(wake_ctx)` when you have data" obligation.
Phase 104.C.6.b already shipped the corresponding vtable slot
(under the name `set_wake_signal`) + per-backend hook plumbing
in Zenoh + DDS; Phase 124.B evolves that slot and adds the
condvar-blocked wait layer that turns the existing flag-based
"poll less often" into condvar-based "wait until signaled."

## Design

### 1 — Unified zero-copy ABI

Add 5 vtable slots:

```c
typedef struct nros_rmw_vtable_t {
    /* ... existing 23 slots ... */

    /* ============ Phase 124 — zero-copy ABI ============
     * NULL slot = backend doesn't lend; runtime emits arena
     * fallback (single memcpy at commit). Lifetime contract:
     * `out_buf` is valid until commit/discard/release runs.
     * Opaque `token` wraps backend's per-loan state (zenoh
     * bytes_t, DDS sample-info, XRCE output cursor). */

    /* Publisher side. */
    nros_rmw_ret_t (*pub_loan)(
        nros_rmw_publisher_t *pub,
        size_t                requested_len,
        uint8_t             **out_buf,
        size_t               *out_cap,        /* may exceed requested */
        void                **out_token);
    nros_rmw_ret_t (*pub_commit)(
        nros_rmw_publisher_t *pub,
        void                 *token,
        size_t                actual_len);
    void           (*pub_discard)(
        nros_rmw_publisher_t *pub,
        void                 *token);

    /* Subscriber side. */
    int32_t (*sub_borrow)(                     /* returns len ≥ 0 or negative ret */
        nros_rmw_subscriber_t *sub,
        const uint8_t        **out_buf,
        size_t                *out_len,
        void                 **out_token);
    void    (*sub_release)(
        nros_rmw_subscriber_t *sub,
        void                  *token);
} nros_rmw_vtable_t;
```

**Rust integration.** `CffiPublisher` implements `SlotLending`:

```rust
impl SlotLending for CffiPublisher {
    type Slot<'a> = CffiSlot<'a>;
    fn try_lend_slot(&self, len: usize) -> Result<Option<CffiSlot<'_>>, TransportError> {
        let vt = active_vtable();
        if vt.pub_loan.is_null() { return Ok(None); }   /* arena fallback */
        /* call vt.pub_loan, wrap raw ptr + token in CffiSlot<'a> */
    }
    fn commit_slot(&self, slot: CffiSlot<'_>) -> Result<(), TransportError> { ... }
}

pub struct CffiSlot<'a> {
    buf: *mut u8, cap: usize, cursor: usize,
    token: *mut c_void,
    publisher: &'a CffiPublisher,
    _life: PhantomData<&'a mut ()>,
}

impl Drop for CffiSlot<'_> {
    fn drop(&mut self) {
        /* User dropped without commit → discard. */
        unsafe { (active_vtable().pub_discard)(self.publisher.handle(), self.token); }
    }
}
```

Rust users keep `try_lend_slot()` / `commit_slot()` ergonomics
unchanged. C users get the same surface via:

```c
uint8_t *buf;
size_t cap;
void *token;
nros_rmw_publisher_loan(pub, 256, &buf, &cap, &token);
encode_into(buf, my_msg);     /* zero-copy */
nros_rmw_publisher_commit(pub, token, encoded_len);
```

**Arena fallback** moves from `nros-node` into `nros-rmw-cffi`. When
`vt.pub_loan == NULL`, the runtime allocates a per-publisher
`TxArena` slot inside `nros_rmw_publisher_t` and emits the
fallback loan path. Same fallback for ALL callers (Rust + C/C++),
not Rust-only.

### 2 — Wake-callback + condvar layer

**Builds incrementally on Phase 104.C.6.b** (already landed,
commit `4c5cb87f`). C.6.b shipped the wake-flag half:

- Vtable slot `set_wake_signal(session, *flag)`.
- Backend stores the flag ptr; writes `1` on async wake.
- `Executor.wake_flag: Arc<AtomicBool>` field.
- `spin_once` swap-clears the flag on entry; primary's
  `drive_io` collapses to 0-ms when flag was set.

Result: multi-session worst-case wake reduced from
N×timeout_ms to 1×timeout_ms. Single-session wake latency
still bounded by `primary_timeout` because the flag write
doesn't interrupt an in-progress `drive_io` block.

Phase 124.B adds the condvar half ON TOP:

- Vtable slot signature change: `set_wake_signal(*flag)` →
  `set_wake_callback(cb, ctx)`.
- Backend stores `(cb, ctx)`; calls `cb(ctx)` on async wake
  instead of writing the flag directly.
- Runtime-supplied `cb` does flag-write + condvar-signal
  atomically.
- `spin_once` blocks on the condvar (with deadline) instead of
  calling `drive_io(primary_timeout)`. All sessions drain
  with `drive_io(0)` after wake.

```c
/* Phase 124.B — replaces phase 104.C.6.b's flag-only slot. */
typedef void (*nros_rmw_wake_cb)(void *ctx);

typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Backend stores (cb, ctx); calls `cb(ctx)` on async wake.
     * `cb == NULL` clears any previously installed callback —
     * backend must drop the stored pair and never invoke after
     * this returns. */
    nros_rmw_ret_t (*set_wake_callback)(
        nros_rmw_session_t *session,
        nros_rmw_wake_cb    cb,
        void               *ctx);
} nros_rmw_vtable_t;
```

**Executor state evolution:**

```rust
pub struct Executor {
    /* ... existing fields ... */

    /* Already present from C.6.b: */
    pub(crate) wake_flag: Arc<AtomicBool>,

    /* Phase 124.B adds: */
    pub(crate) wake_cv:    Arc<Condvar>,
    pub(crate) wake_mu:    Arc<Mutex<()>>,
}
```

**Combined wake callback (runtime-supplied):**

```rust
extern "C" fn nros_rmw_runtime_wake_cb(ctx: *mut c_void) {
    // ctx is &Executor opaque. Idempotent + ISR-safe.
    let exec = unsafe { &*(ctx as *const Executor) };
    exec.wake_flag.store(true, SeqCst);
    let _g = exec.wake_mu.lock();          // ISR-safe variants on RTOS
    nros_platform_condvar_signal(exec.wake_cv.as_raw());
}
```

**spin_once becomes:**

```rust
fn spin_once(&mut self, timeout: Duration) -> SpinOnceResult {
    let deadline_ns = self.platform.clock_ns()
        + timeout.as_nanos().min(i32::MAX as u128) as i64;

    // Block on condvar until wake_flag set or deadline.
    {
        let mut g = self.wake_mu.lock();
        while !self.wake_flag.swap(false, SeqCst)
              && self.platform.clock_ns() < deadline_ns
        {
            g = self.wake_cv.wait_until(g, deadline_ns);
        }
    }

    // All sessions non-blocking drain — backends just dequeue
    // whatever their worker / poll path already enqueued.
    let _ = self.session.drive_io(0);
    for extra in self.extra_sessions.iter_mut() {
        let _ = extra.drive_io(0);
    }

    self.run_ready()
}
```

**Platform mapping (already shipped via Phase 121):**

| Platform | wake_cv impl | wait_until | signal |
|---|---|---|---|
| POSIX | `pthread_cond_t` | `pthread_cond_timedwait` | `pthread_cond_signal` |
| Zephyr | `k_condvar` | `k_condvar_wait_timeout` | `k_condvar_signal` |
| FreeRTOS | binary semaphore + mutex | `xSemaphoreTake(timeout)` | `xSemaphoreGiveFromISR` |
| NuttX | `sem_t` (pthread condvar broken, see CLAUDE.md) | `sem_timedwait` | `sem_post` |
| ThreadX | `tx_event_flags` group | `tx_event_flags_get(TX_OR, timeout)` | `tx_event_flags_set` |
| Bare-metal | counter + WFI spin | `while (!flag && !deadline) { wfi(); }` | atomic store |

All exposed via `nros_platform_condvar_*` from the Phase 121
platform ABI. No new platform primitive needed.

**Guard condition (C user-facing primitive):**

```c
typedef struct nros_guard_condition_t {
    /* Opaque — runtime stores an &Executor reference. */
    void *_internal;
} nros_guard_condition_t;

void nros_guard_condition_trigger(nros_guard_condition_t *g) {
    /* Same wake path backends use. Idempotent. ISR-safe. */
    nros_rmw_runtime_wake_cb(g->_internal);
}
```

ISR-safe contract: `nros_rmw_runtime_wake_cb` MUST be callable
from interrupt context. RTOS impls use ISR-safe variants
(`xSemaphoreGiveFromISR`, `tx_event_flags_set` from ISR).
Documented as a runtime contract; backends don't need to know.

**Poll-only backends without a worker thread** (single-thread
XRCE, bare-metal): `set_wake_callback` slot can be NULL. Runtime
falls back to the 1×timeout_ms behaviour from C.6.b (drive_io
called with the user's timeout; flag-on-entry short-circuit
still works). Lossless degradation.

**Backend obligations** (single-line, per session):

| Backend shape | Pre-104.C.6.b | Post-104.C.6.b | Post-124.B |
|---|---|---|---|
| Has own I/O thread (zenoh-pico, dust-DDS POSIX) | none | `*self.flag = 1` after enqueue | `(self.wake_cb)(self.wake_ctx)` after enqueue |
| Poll-only (XRCE, bare-metal) | drive_io blocks user timeout | same + writes flag if data | option to keep flag-only (NULL callback) or implement same as above |
| External event (ISR, signal handler) | user calls `executor.halt()` | same | calls `nros_guard_condition_trigger` |

### 3 — Service availability probe

```c
typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Phase 124 — service availability probe.
     * Returns 1 if ≥ 1 matching server discovered, 0 if none,
     * negative `nros_rmw_ret_t` on error. NULL slot = backend
     * cannot answer; runtime returns NROS_RMW_RET_UNSUPPORTED. */
    int32_t (*service_server_available)(
        nros_rmw_service_client_t *client);
} nros_rmw_vtable_t;
```

Backend impl notes:
- **Zenoh**: `z_session` tracks matched queryables via interest
  declarations. Implementable.
- **DDS** (Cyclone, dust-dds): `DataReader::get_matched_publications`
  / `BuiltinTopicData` discovery.
- **XRCE**: returns `RET_UNSUPPORTED` — `micro-XRCE-DDS-Client`
  has no participant enumeration. Match micro-ROS's behaviour.

### 4 — Sequence take

```c
typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Phase 124 — batch take. Returns count of messages
     * taken (0..max), or negative `nros_rmw_ret_t` on error.
     * `buf` is a contiguous block of `max_msgs * per_msg_cap`
     * bytes; the i-th message lives at `buf + i*per_msg_cap`
     * with length `out_lens[i]`. NULL slot = backend doesn't
     * batch; runtime emits a `try_recv_raw` loop fallback. */
    int32_t (*try_recv_sequence)(
        nros_rmw_subscriber_t *sub,
        uint8_t               *buf,
        size_t                 per_msg_cap,
        size_t                 max_msgs,
        size_t                *out_lens);
} nros_rmw_vtable_t;
```

Backend impl notes:
- **Zenoh**: backend already enqueues into per-sub queue; batch
  drain.
- **DDS**: `dds_take` (Cyclone) / `take(max_samples)` (Fast DDS /
  dust-dds) is native.
- **XRCE**: best-effort drain from session input buffer.

### 5 — Continuous serialization

```c
/* Forward declarations. */
typedef void (*nros_rmw_serialize_size_cb)(
    size_t *out_total_len, void *user_ctx);
typedef void (*nros_rmw_serialize_chunk_cb)(
    uint8_t *out_buf, size_t cap, size_t *out_written,
    void *user_ctx);

typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Phase 124 — streamed publish. Backend calls `size_cb` to
     * learn the total payload length, allocates one slot of
     * that size in its outbound buffer, then calls
     * `chunk_cb` repeatedly to fill chunks until the buffer is
     * full. Saves a staging buffer for big messages on RAM-
     * constrained nodes. NULL slot = backend doesn't stream;
     * runtime falls back to user-provided one-shot
     * publish_raw using a staging buffer. */
    nros_rmw_ret_t (*publish_streamed)(
        nros_rmw_publisher_t       *pub,
        nros_rmw_serialize_size_cb  size_cb,
        nros_rmw_serialize_chunk_cb chunk_cb,
        void                       *user_ctx);
} nros_rmw_vtable_t;
```

Lesson from `rmw_uros_set_continous_serialization_callbacks`.
Cleaner ABI: pass callbacks per-call instead of binding them to
the publisher state, so different messages on the same publisher
can use different serialisation strategies.

### 6 — Ping primitive

```c
typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Phase 124 — session-level connectivity probe. Returns
     * RET_OK if the peer/agent responded within timeout_ms,
     * RET_TIMEOUT otherwise, RET_UNSUPPORTED if backend can't
     * probe. Less work than service-availability probe — no
     * discovery state required, just a wire-level round trip
     * (ICMP-like). */
    nros_rmw_ret_t (*ping_session)(
        nros_rmw_session_t *session,
        int32_t             timeout_ms);
} nros_rmw_vtable_t;
```

Backend impl notes:
- **Zenoh**: `z_send_ping` (or session keep-alive piggyback).
- **DDS**: round-trip on the built-in service discovery channel.
- **XRCE**: `uxr_ping_agent_session_until_timeout`. Maps directly.

### Vtable surface after Phase 124

| Slot count | Today (post-104.C.6.b) | After 124 |
|---|---|---|
| Total slots | 24 (23 original + `set_wake_signal`) | 33 (+9 net; `set_wake_signal` renamed to `set_wake_callback`, +1 compat alias optional) |
| Required (non-NULL) | 23 | 23 (unchanged — all new slots optional) |
| Optional | 1 | 10 |

Backwards-compatibility: every new slot is optional. Existing
backends keep working unchanged. Backends opt in incrementally.

## Work items

Order: zero-copy + dispatch first (P1), the rest follow as
independent additions.

### Thread A — Zero-copy ABI

- [x] **124.A.1 — vtable slot additions.** Add the 5 zero-copy
      slots to `nros_rmw_vtable_t` + matching docs. **Done.**
      Header: `pub_loan / pub_commit / pub_discard / sub_borrow /
      sub_release`. Rust mirror in `NrosRmwVtable`. All existing
      vtable instantiations (RustBackendAdapter, two_backends test
      stubs, typed_struct test stub, XRCE C vtable, Cyclone DDS
      C++ vtable) carry None/NULL slots.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/src/rust_adapter.rs`,
      `packages/core/nros-rmw-cffi/tests/two_backends.rs`,
      `packages/xrce/nros-rmw-xrce/src/vtable.c`,
      `packages/dds/nros-rmw-cyclonedds/src/vtable.cpp`.
- [x] **124.A.2 — CffiPublisher SlotLending impl.** Wrap the
      vtable slots in Rust's `SlotLending` trait.
      `CffiSubscriber: SlotBorrowing` mirror. **Done.** New
      `CffiSlot<'a>` + `CffiView<'a>` types. Drop fires
      `pub_discard` / `sub_release`; commit/release cancel the
      drop via `Option<&_>::take()`. Backends with NULL slots
      return `Ok(None)` from `try_lend_slot` / `try_borrow` so
      callers fall back to the arena path (124.A.3).
      Gated behind a new `lending` feature on `nros-rmw-cffi`
      that forwards to `nros-rmw/lending`.
      **Files:** `packages/core/nros-rmw-cffi/Cargo.toml`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.
- [x] **124.A.3 — Arena fallback migration.** Move `TxArena<TX_BUF>`
      from `nros-node` into `nros-rmw-cffi` as the default loan
      path when `vt.pub_loan == NULL`. Single implementation
      serves Rust + C/C++.
      **Done.** Implemented in `CffiPublisher::try_lend_slot`:
      when `vt.pub_loan` is None, allocates an `ArenaStaging`
      Box (Vec-backed staging buffer) under `feature = "alloc"`
      and stashes it in the slot's `token`. `commit_slot` on a
      fallback slot reclaims the Box and emits a single
      `publish_raw` of the cursor-truncated bytes. `Drop` on a
      fallback slot reclaims the Box without sending. no_std-
      no_alloc builds return `Ok(None)` so callers fall back
      further. Per-publisher TX-arena in nros-node stays for
      now — Phase 99.F retains it for users who hit the path
      directly; the cffi-side fallback covers C/C++ + the
      generic Rust route via `CffiPublisher`.
      Tests: `tests/loan_fallback.rs` — two nextest scenarios
      cover commit + drop-without-commit.
      **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/tests/loan_fallback.rs`.
- [~] **124.A.4 — Zenoh-pico backend wire-up.** Map
      `pub_loan` → `zp_alloc_pub_payload`, `pub_commit` → put,
      etc. Delete the legacy `nros-rmw-zenoh::shim::publisher::
      SlotLending` impl (now redundant — vtable path covers it).
      **Partial.** ZenohPublisher's existing Rust-side
      SlotLending (Phase 99.F, single-slot arena with
      `publish_with_attachment_aliased`) already provides
      zero-copy publish for direct Rust callers. Wiring it
      through the cffi vtable's `pub_loan/commit/discard`
      trampolines so C/C++ callers get native zenoh zero-copy
      is a follow-up (124.A.4.b) — the arena fallback in
      124.A.3 covers them today with a single memcpy.
      **Files:** `packages/zpico/nros-rmw-zenoh/src/shim/`.
- [x] **124.A.5 — XRCE / DDS / Cyclone backend stubs.** All
      three set the slots to NULL initially (arena fallback
      covers). Cyclone DDS native loan (`dds_loan_sample`) wired
      in a follow-up.
      **Done.** XRCE C vtable, Cyclone DDS C++ vtable, and
      both Rust adapters (RustBackendAdapter via dust-DDS +
      zenoh) ship the 5 zero-copy slots as NULL / nullptr.
- [~] **124.A.6 — C user-facing wrappers.** Add
      `nros_publisher_loan` / `commit` / `discard` to the C
      header that the cbindgen `nros_generated.h` exports.
      Same for subscriber borrow / release.
      **Publisher half done.** `nros_publisher_loan` /
      `nros_publisher_commit` / `nros_publisher_discard` added
      to `nros-c/src/publisher.rs`, gated behind the new
      `lending` cargo feature on `nros-c`. Token =
      `Box::into_raw(Box<CffiSlot<'static>>)` with the
      lifetime erased; caller MUST commit / discard before
      finalising the publisher (documented in the doc-comment
      contract). cbindgen emits `nros_publisher_loan` /
      `_commit` / `_discard` into `nros_generated.h`.
      Subscription borrow / release pending (follow-up).
      **Files:** `packages/core/nros-c/src/publisher.rs`,
      `packages/core/nros-c/Cargo.toml`,
      `packages/core/nros/Cargo.toml`,
      `packages/core/nros/src/lib.rs` (RmwSlot / RmwView
      type aliases in `internals`).
- [ ] **124.A.7 — C++ user-facing class methods.**
      `Publisher<M>::loan(len)` / `commit(slot)` /
      `Subscription<M>::borrow()` / `release(view)` matching
      Rust's API shape.
      **Files:** `packages/core/nros-cpp/include/nros/publisher.hpp`,
      `packages/core/nros-cpp/include/nros/subscription.hpp`.
- [ ] **124.A.8 — Loaned message E2E test.** Verifies (a) Rust
      and C produce byte-identical wire output when both use
      loan; (b) C user calling loan + commit on zenoh-pico
      backend produces zero-copy publish (verifiable via
      malloc-trace hook); (c) arena fallback works on
      non-lending backends.
      **Files:** `packages/testing/nros-tests/tests/loan_zero_copy.rs`.

### Thread B — Wake-callback + condvar layer

Builds incrementally on phase 104.C.6.b's flag mechanism;
~50 LOC total across runtime + 2 backends. No new standalone
`nros_rmw_dispatch_t` struct — the dispatch state fields fold
into `Executor` alongside the existing `wake_flag`.

- [ ] **124.B.1 — Vtable slot signature change.** Rename
      `set_wake_signal(session, *flag)` → `set_wake_callback(
      session, cb, ctx)`. Backend stores `(cb, ctx)` instead of
      `*flag`; calls `cb(ctx)` on async wake.
      Compatibility: keep the old `set_wake_signal` slot for one
      release cycle as a no-op alias that internally builds a cb
      closure writing to the supplied `*flag`. Delete in a
      follow-up once both impls (Zenoh + DDS) migrate.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/src/rust_adapter.rs`.

- [ ] **124.B.2 — Executor wake_cv + wake_mu fields.** Add
      `wake_cv: Arc<Condvar>` + `wake_mu: Arc<Mutex<()>>` next to
      the existing `wake_flag: Arc<AtomicBool>` from C.6.b.
      Define `nros_rmw_runtime_wake_cb(ctx)` runtime API that
      writes `wake_flag = 1` + signals `wake_cv` atomically.
      Implement via `nros_platform_condvar_*` from Phase 121.
      **Files:** `packages/core/nros-node/src/executor/spin.rs`
      (struct), `packages/core/nros-rmw-cffi/src/lib.rs`
      (runtime cb).

- [ ] **124.B.3 — Backend migration to callback (2 backends).**
      ZenohSession + DdsSession replace their flag-write line
      with `(self.wake_cb)(self.wake_ctx)`. XRCE + Cyclone leave
      slot NULL (poll-only, lossless degradation to C.6.b
      flag-only path via the compatibility alias).
      **Files:**
      `packages/zpico/nros-rmw-zenoh/src/shim/session.rs`,
      `packages/dds/nros-rmw-dds/src/session.rs`.

- [ ] **124.B.4 — Executor spin refactor.** Replace today's
      `self.session.drive_io(primary_timeout)` blocking call
      with a `wake_cv.wait_until(deadline)` then
      `drive_io(0)` non-blocking drain on every session.
      Existing wake_flag swap-clear stays as the wake-pending
      check inside the cv-wait loop.
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [ ] **124.B.5 — Guard condition C API.** Expose
      `nros_guard_condition_create(executor)`,
      `_trigger(g)`, `_destroy(g)`. Stores an opaque
      &Executor ref; `_trigger` calls
      `nros_rmw_runtime_wake_cb` on that ref.
      **Files:** `packages/core/nros-c/src/guard_condition.rs`
      (new), header export.

- [ ] **124.B.6 — Guard condition C++ class.** Phase 122 already
      shipped `nros::GuardCondition` Rust-side; expose the C
      surface via the cbindgen header + add a thin C++ wrapper
      class.
      **Files:** `packages/core/nros-cpp/include/nros/guard_condition.hpp`.

- [ ] **124.B.7 — ISR-safe wake contract.** Document that
      `nros_rmw_runtime_wake_cb` is ISR-safe; verify the
      platform condvar impl chooses ISR-safe variants
      (`xSemaphoreGiveFromISR`, `tx_event_flags_set` from
      ISR). Test: signal handler (POSIX) + Cortex-M timer ISR
      wake the executor within one tick.
      **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/testing/nros-tests/tests/dispatch_isr_wake.rs`.

- [ ] **124.B.8 — Wake-latency measurement.** Microbenchmark
      sub-receive → callback-run latency: pre-124.B (drive_io
      timeout-bound) vs post-124.B (condvar-bound). Target:
      ≥ 10× improvement on POSIX, P99 ≤ 100 µs on Cortex-M3
      QEMU + zenoh-pico. Feeds Phase 110 PiCAS test budget.
      **Files:**
      `packages/testing/nros-tests/tests/wake_latency_bench.rs`.

### Thread C — Service availability probe

- [ ] **124.C.1 — vtable slot.** Add
      `service_server_available` to vtable + Rust trait.
- [ ] **124.C.2 — Backend impls.** Zenoh (queryable interest),
      Cyclone DDS (matched-pub), dust-dds (DataReader API).
      XRCE returns `RET_UNSUPPORTED`.
- [ ] **124.C.3 — C/C++ wrappers.** `nros_client_server_available`
      + `Client<S>::server_available()`.
- [ ] **124.C.4 — Test.** Client spawned before server; probe
      returns 0; spawn server; probe returns 1.

### Thread D — Sequence take

- [ ] **124.D.1 — vtable slot.** Add `try_recv_sequence`.
- [ ] **124.D.2 — Loop fallback.** Runtime emits a
      `try_recv_raw` loop when `vt.try_recv_sequence == NULL`.
- [ ] **124.D.3 — Backend impls.** Zenoh batch drain, Cyclone
      `dds_take(max_samples)`, dust-dds equivalent.
- [ ] **124.D.4 — C/C++ wrappers + test.** Test verifies 8
      messages drained in one call delivers all 8 + correct
      lengths.

### Thread E — Continuous serialization

- [ ] **124.E.1 — vtable slot + callback typedefs.**
- [ ] **124.E.2 — Staging-buffer fallback.** Runtime calls
      `size_cb` then `chunk_cb` into a stack staging buffer
      (capped at NROS_MAX_STREAM_CHUNK), then `publish_raw`.
- [ ] **124.E.3 — Zenoh + XRCE backend impls.** Zenoh: write
      into network buffer directly. XRCE: use micro-CDR
      streaming write APIs.
- [ ] **124.E.4 — User-facing API.** `Publisher<M>::publish_streamed(|writer| {
      ... })` taking a Rust `FnOnce(StreamWriter)`. C
      equivalent: pair of callbacks.

### Thread F — Ping primitive

- [ ] **124.F.1 — vtable slot.** Add `ping_session`.
- [ ] **124.F.2 — Backend impls.** Zenoh `z_send_ping`, XRCE
      `uxr_ping_agent_session_until_timeout`. DDS:
      RET_UNSUPPORTED unless built-in topics light up.
- [ ] **124.F.3 — C/C++ + Rust API.**
      `nros_session_ping(session, timeout_ms)`,
      `Executor::ping(timeout)`.
- [ ] **124.F.4 — Test.** Bring up agent → ping succeeds.
      Tear down agent → ping returns RET_TIMEOUT within
      configured timeout.

## Acceptance criteria

### Thread A — Zero-copy

- [ ] `Publisher<M>::loan(len)` in C++ returns a writable slot
      on zenoh-pico + `rmw-lending`; produces a wire packet
      with ZERO heap allocations (verified via malloc trace).
- [ ] Same call on dust-dds returns slot via arena fallback;
      verifiable single memcpy at commit (one alloc → one
      free per loan cycle).
- [ ] Rust + C produce byte-identical CDR output when both
      take the loan path with the same payload.
- [ ] `cargo test -p nros-tests --test loan_zero_copy` green.

### Thread B — Wake-callback + condvar

- [ ] Executor spin with 4 idle subscribers + 1-Hz timer wakes
      exactly N times per N seconds (no busy poll, no missed
      wakes).
- [ ] ISR-safe wake test: signal handler calls
      `nros_rmw_runtime_wake_cb` (or
      `nros_guard_condition_trigger`); executor unblocks
      within 1 ms of the signal (POSIX) / 1 tick (FreeRTOS QEMU).
- [ ] Wake-latency P99 (subscriber-receive → callback-run) ≤ 100 µs
      on Cortex-M3 QEMU + zenoh-pico. Compare ≥ 10× improvement
      over current C.6.b flag-only path.
- [ ] Backwards-compat: backend with NULL `set_wake_callback`
      slot continues to work via C.6.b flag-only path; no
      regression in single-backend / single-Executor builds.
- [ ] Multi-RMW bridge: pubs on backend A receive ≥ 99% of
      messages within `condvar_wake_latency + drive_io_drain`
      budget when backend B is idle.

### Thread C — Service available

- [ ] `Client<S>::server_available()` returns false before
      server is up, true after, within 100 ms of server's
      first publish-discovery.
- [ ] XRCE backend returns `RET_UNSUPPORTED` cleanly.

### Thread D — Sequence take

- [ ] `try_recv_sequence(8)` on a sub with 8 queued messages
      returns 8 with correct per-message lengths in one call.
- [ ] Fallback loop produces same result on backends without
      the slot.

### Thread E — Continuous serialization

- [ ] Streamed publish of a 4 KB message uses ≤ 256 B of
      stack staging on a backend that supports streaming; ≤
      4 KB on fallback path.
- [ ] Wire output byte-identical to one-shot `publish_raw`.

### Thread F — Ping

- [ ] Ping returns RET_OK within 50 ms when agent is up; ≥
      configured timeout_ms when down.

## Memory + code-size budget

| Thread | Vtable slots | Runtime size | Per-entity overhead |
|---|---|---|---|
| A — zero-copy | +5 | ~0.5 KB (arena fallback) | +1 fn ptr per entity struct |
| B — wake-callback + cv | 0 net (renamed C.6.b slot) | ~128 B (cv + mu in Executor; flag from C.6.b stays) | 0 |
| C — service available | +1 | 0 | 0 |
| D — sequence take | +1 | 0 (loop fallback) | 0 |
| E — continuous ser | +1 | ≤256 B staging | 0 |
| F — ping | +1 | 0 | 0 |
| **Total** | **+9** | **~900 B** | **+5 bytes** |

Vtable struct grows from 24 → 33 fn ptrs (≈ 192 → 264 bytes on
64-bit; one VTABLE singleton per backend). Negligible.

## Notes

- **Phase 110 interaction.** Phase 110 (PiCAS, scheduling
  context) needs sub-poll-period wake latency. Thread B
  delivers it. The two phases can develop in parallel; final
  PiCAS handoff test verifies Thread B's wake-latency goal.
- **Phase 99 deprecation.** Phase 99 spec'd the loan ABI but
  never shipped. Phase 124.A supersedes it. The
  `can_loan_messages` flag on entity structs becomes
  redundant (callers query `vt.pub_loan != NULL`); keep the
  field for one release cycle for backward source-compat then
  delete in a future cleanup.
- **`rmw-lending` Cargo feature.** Becomes unconditional under
  Phase 124 — the cffi vtable path is the universal loan API.
  Feature flag removed in 124.A.4.
- **Backend-author docs.** Each new optional slot gets a
  one-paragraph "what to implement" + "when to return NULL"
  entry in `book/src/internals/rmw-backends.md`.
- **micro-ROS lessons folded in.** Continuous serialization (E)
  + ping (F) come directly from `rmw_microros/*.h` extensions.
  Custom transport already handled by Phase 115.B
  `set_custom_transport`. Discovery deliberately not adopted
  (won't-do per coverage doc).
- **Honest coverage statement.** After Phase 124 lands, nano-ros
  data-plane coverage rises from ~30-35% of upstream rmw.h
  surface to ~55-60%. The remaining gap (graph introspection,
  GIDs, content filter, network flow, allocation hooks) stays
  won't-do.

## Stream order

Recommended landing order:

1. **A** (zero-copy) — unblocks C/C++ zero-copy story; biggest
   surface change.
2. **B** (dispatch) — unblocks Phase 110 PiCAS work; biggest RT
   payoff.
3. **C** + **D** + **F** — small mechanical additions, can
   parallelize.
4. **E** — continuous serialization is bigger lift; lands when
   memory-budgeted nodes need it.

A and B are independent and can proceed in parallel.

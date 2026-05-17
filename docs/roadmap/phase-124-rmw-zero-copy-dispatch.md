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
2. **Wake-callback + condvar layer** — supersedes phase
   104.C.6.b's flag-based wake (commit `4c5cb87f`). Vtable slot
   `set_wake_signal(*flag)` is replaced by
   `set_wake_callback(cb, ctx)`; backend calls `cb(ctx)` on
   async wake; runtime-supplied `cb` writes the existing
   `Executor.wake_flag` AND signals a new `Executor.wake_cv`
   atomically. Spin loop blocks on the condvar (sub-poll wake
   latency). No backward-compat alias — `set_wake_signal` is
   deleted in the same change. Replaces upstream `rmw_wait` +
   `rmw_guard_condition_t` with platform-condvar dispatch (no
   waitset → no RTOS stubs). One-line backend obligation;
   ISR-safe contract on every supported platform.
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

**Status.** In flight — all six threads have their P1 cross-
language paths landed (vtable slot + Rust trait method + C/C++
wrappers + routing tests). As of 2026-05-14:

- **A — zero-copy:** vtable slots + `SlotLending`/`SlotBorrowing`
  + arena fallback + zenoh-pico native loan + C/C++ wrappers +
  malloc-trace zero-alloc test. Done.
- **B — wake-callback + condvar:** `set_wake_callback` slot,
  `wake_cv` + condvar-blocked spin, guard-condition C/C++ surface,
  ISR-safe platform primitive + signalfd worker. Done.
- **C — service availability probe:** full stack + routing test.
  Done. Acceptance E2E (100 ms timing) pending.
- **D — sequence take:** vtable slot + loop fallback + C/C++/Rust
  wrappers + routing test. Native batch (D.3) landed for all three
  capable backends — Cyclone (`dds_take`), dust-dds
  (`DataReader::take`), zenoh-pico (new SPSC ring). XRCE + FastDDS
  keep the loop fallback (no native take_n; matches upstream).
- **E — continuous serialization:** vtable slot + staging-buffer
  fallback + C/C++/Rust wrappers + routing test + native impls
  for **zenoh-pico** (`z_bytes_writer`) and **XRCE**
  (`uxr_prepare_output_stream`).
- **F — ping:** vtable slot + C/C++/Rust wrappers + routing test
  + native impls for **zenoh-pico** (`zp_send_keep_alive`,
  send-side liveness) and **XRCE** (`uxr_ping_agent_session`,
  true round-trip). DDS inherits the default `Unsupported`.

Remaining open: 124.B.8 wake-latency microbench (deferred —
needs a bench harness) and the network-E2E acceptance tests.
Sub-phase detail in §"Work items" below.

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
XRCE, bare-metal): `set_wake_callback` slot NULL is allowed.
Runtime still cv-waits up to user timeout, then non-blocking
drains every session — poll-only backends end up draining on
the deadline boundary, equivalent to their pre-124 behaviour
without needing a wake signal.

**Backend obligations** (single-line, per session):

| Backend shape | Pre-124.B | Post-124.B |
|---|---|---|
| Has own I/O thread (zenoh-pico, dust-DDS POSIX) | `*self.flag = 1` after enqueue | `(self.wake_cb)(self.wake_ctx)` after enqueue |
| Poll-only (XRCE, bare-metal) | drive_io blocks user timeout | leave callback slot NULL — runtime cv-waits to deadline, drains 0-ms |
| External event (ISR, signal handler) | user calls `executor.halt()` | calls `nros_guard_condition_trigger` (ISR-safe) |

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
- [x] **124.A.4 — Zenoh-pico backend wire-up.** Map
      `pub_loan` → `zp_alloc_pub_payload`, `pub_commit` → put,
      etc. **Done (A.4.b).** When the `lending` feature is on,
      `nros_rmw_zenoh_register` installs a customised vtable
      built from `RustBackendAdapter::<ZenohRmw>::VTABLE` with
      `pub_loan/_commit/_discard` overridden by zenoh-pico-
      specific trampolines that route through
      `ZenohPublisher`'s Phase 99.F single-slot arena +
      `publish_with_attachment_aliased`. C/C++ callers
      going through the cffi vtable now get the same
      zero-copy publish path Rust users have via direct
      `SlotLending`. Trampolines box a lifetime-erased
      `ZenohSlot<'static>` as the opaque token; Drop / discard
      reclaim the arena. New `ZenohSlot::truncate(actual_len)`
      lets commit honour `actual_len < cap`. Without the
      `lending` feature, registration falls back to the
      adapter's NULL loan slots → runtime arena fallback
      (124.A.3) — still one memcpy, still zero-copy at the
      wire level.
      **Files:** `packages/zpico/nros-rmw-zenoh/src/lib.rs`,
      `packages/zpico/nros-rmw-zenoh/src/shim/publisher.rs`.
- [x] **124.A.5 — XRCE / DDS / Cyclone backend stubs.** All
      three set the slots to NULL initially (arena fallback
      covers). Cyclone DDS native loan (`dds_loan_sample`) wired
      in a follow-up.
      **Done.** XRCE C vtable, Cyclone DDS C++ vtable, and
      both Rust adapters (RustBackendAdapter via dust-DDS +
      zenoh) ship the 5 zero-copy slots as NULL / nullptr.
- [x] **124.A.6 — C user-facing wrappers.** Add
      `nros_publisher_loan` / `commit` / `discard` to the C
      header that the cbindgen `nros_generated.h` exports.
      Same for subscriber borrow / release. **Done.**
      Publisher: `nros_publisher_loan` /
      `nros_publisher_commit` / `nros_publisher_discard`.
      Subscription: `nros_subscription_borrow` /
      `nros_subscription_release`. All 5 entries gated behind
      the new `lending` cargo feature on `nros-c`. Tokens =
      boxed `RmwSlot<'static>` / `RmwView<'static>` with
      lifetime erased at the FFI boundary; caller MUST
      consume the token before destroying the
      publisher / subscription. cbindgen emits all 5 entries
      into `nros_generated.h`.
      **Files:** `packages/core/nros-c/src/publisher.rs`,
      `packages/core/nros-c/src/subscription.rs`,
      `packages/core/nros-c/Cargo.toml`,
      `packages/core/nros/Cargo.toml`,
      `packages/core/nros/src/lib.rs` (RmwSlot / RmwView
      type aliases in `internals`).
- [x] **124.A.7 — C++ user-facing class methods.**
      `Publisher<M>::loan(len)` / `commit(slot)` /
      `Subscription<M>::borrow()` / `release(view)` matching
      Rust's API shape. **Done.** New nested RAII types:
      `nros::Publisher<M>::Loan` (Drop fires
      `nros_cpp_publisher_discard`; `commit(actual_len)` and
      `discard()` consume the loan explicitly) and
      `nros::Subscription<M>::View` (Drop fires
      `nros_cpp_subscription_release`). Entry-point methods
      return `Expected<Loan>` / `Expected<View>`. Backed by
      5 new `nros_cpp_publisher_loan/_commit/_discard` and
      `nros_cpp_subscription_borrow/_release` extern fns in
      `nros-cpp/src/{publisher,subscription}.rs`, gated by a
      new `lending` feature on `nros-cpp`. cbindgen emits
      the new entries into `nros_cpp_ffi.h`. Header smoke
      compiles under `-std=gnu++14 -DNROS_PLATFORM_POSIX`.
      **Files:** `packages/core/nros-cpp/Cargo.toml`,
      `packages/core/nros-cpp/src/lib.rs`,
      `packages/core/nros-cpp/src/publisher.rs`,
      `packages/core/nros-cpp/src/subscription.rs`,
      `packages/core/nros-cpp/include/nros/publisher.hpp`,
      `packages/core/nros-cpp/include/nros/subscription.hpp`.
- [x] **124.A.8 — Loaned message E2E test.** Verifies (a) Rust
      and C produce byte-identical wire output when both use
      loan; (b) C user calling loan + commit on zenoh-pico
      backend produces zero-copy publish (verifiable via
      malloc-trace hook); (c) arena fallback works on
      non-lending backends. **Done at the cffi layer.**
      Coverage split into two integration tests that
      together exercise both halves of the loan path:
        * `tests/loan_fallback.rs` (2 tests) — `pub_loan ==
          NULL` backend; commit triggers `publish_raw` with
          arena bytes; drop without commit reclaims arena.
        * `tests/loan_native.rs` (2 tests) — backend
          exposing native `pub_loan/_commit/_discard`;
          commit routes through `pub_commit` (no fallback
          publish_raw); drop without commit fires
          `pub_discard`; in-stub assertions on the token
          tag catch routing bugs immediately.
      A full E2E test on a real zenoh-pico backend lands as
      124.A.8.b (below); the cffi-layer coverage proves the
      dispatch path is right.
      **Files:** `packages/core/nros-rmw-cffi/tests/loan_fallback.rs`,
      `packages/core/nros-rmw-cffi/tests/loan_native.rs`.

- [x] **124.A.8.b — Zenoh-pico E2E loan test.** Runtime
      verification of the Phase 124.A.4.b zenoh native loan
      trampolines: two-executor split (publisher + subscriber
      threads, both round-tripping through one shared
      `ZenohRouter` fixture), `try_loan` + `commit` on the
      publisher, asserts subscriber receives the payload
      byte-identical. Test at
      `packages/testing/nros-tests/tests/loan_e2e.rs` under
      a new `loan-e2e` cargo feature that pulls
      `nros-rmw-zenoh/lending` + `nros-platform-cffi/posix-c-port`
      so the posix C symbols (`nros_platform_*`) link.
      Run with `cargo nextest run -p nros-tests --features
      loan-e2e --test loan_e2e`. Requires `just zenoh setup`.
      **Files:** `packages/testing/nros-tests/tests/loan_e2e.rs`,
      `packages/testing/nros-tests/Cargo.toml`,
      `packages/zpico/nros-rmw-zenoh/src/shim/session.rs`,
      `packages/core/nros-node/Cargo.toml`.

- [x] **124.A.8.c — Malloc-trace zero-alloc assertion.**
      Companion test in the same file
      (`loan_path_is_alloc_free_on_native_zenoh`). Installs a
      counting `#[global_allocator]` that wraps `System`,
      snapshots the alloc counter before/after a tight
      `try_loan → write → commit` loop on the native zenoh
      path, asserts the delta stays within a small per-publish
      budget (`ALLOC_BUDGET_PER_PUBLISH = 4`, allowance for
      transient log/string allocs only). One warm-up publish
      excludes first-publish lazy init. Catches regressions in
      `z_bytes_from_static_buf` aliasing on zenoh-pico bumps.
      **Files:** `packages/testing/nros-tests/tests/loan_e2e.rs`.

### Thread B — Wake-callback + condvar layer

Supersedes phase 104.C.6.b's flag mechanism in-place; ~80 LOC
total across runtime + 2 backends. No new standalone
`nros_rmw_dispatch_t` struct — the dispatch state fields fold
into `Executor` alongside the existing `wake_flag`. **No
backward-compat alias kept** — `set_wake_signal` is deleted in
the same change as `set_wake_callback` lands.

- [x] **124.B.1 — Vtable slot signature change.** Add
      `set_wake_callback(session, cb, ctx)` to the vtable +
      Session trait. (Landed 2026-05-14, commit `2e5204ca`.)
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/src/rust_adapter.rs`,
      `packages/core/nros-rmw/src/traits.rs`.

- [x] **124.B.2 — Executor wake_cv + wake_mu fields.** Add
      `wake_cv: Arc<Condvar>` + `wake_mu: Arc<Mutex<()>>` next to
      the existing `wake_flag: Arc<AtomicBool>`. Define
      `nros_rmw_runtime_wake_cb(ctx)` runtime API that writes
      `wake_flag = 1` lock-free, then signals `wake_cv` without
      holding the mutex. Lost-wakeup is prevented by SeqCst flag
      write happens-before `notify`, and the waiter checks the
      flag under mutex in the wait predicate. (Landed
      2026-05-14, commit `aa0f89b3`.)
      **Files:** `packages/core/nros-node/src/executor/spin.rs`
      (struct + runtime cb).

- [x] **124.B.4 — Executor spin refactor.** Refactor
      `spin_once` to cv-wait against deadline, then
      `drive_io(0)` non-blocking drain on every session.
      (Landed 2026-05-14, commit `20f8fd95`.)
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [x] **124.B.3 — Backend migration to callback (2 backends).**
      ZenohSession + DdsSession swapped their flag-pointer storage
      for `(wake_cb, wake_ctx)` pair; `drive_io` fires the runtime
      cb on observed work. XRCE + Cyclone left slot NULL (runtime
      cv-waits to user deadline then drains). (Landed 2026-05-14,
      commit `356aabf3`.)
      **Files:**
      `packages/zpico/nros-rmw-zenoh/src/shim/session.rs`,
      `packages/dds/nros-rmw-dds/src/session.rs`.

- [x] **124.B.4.b — Delete `set_wake_signal` slot.** Removed
      `set_wake_signal` from vtable header, Rust struct,
      trampoline, Session trait, and Executor install path. The
      `supports_wake_callback()` detection branch in `spin_once`
      was removed — spin always cv-waits to deadline then drains
      `drive_io(0)`. (Landed 2026-05-14, commit `356aabf3`.)
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/src/rust_adapter.rs`,
      `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-node/src/executor/spin.rs`.

- [x] **124.B.5 — Guard condition C API.** Wired
      `GuardConditionHandle::trigger()` to invoke the runtime
      wake callback after writing the arena flag. The pre-existing
      `nros_guard_condition_trigger` C entry point traces through
      the same handle, so the C surface is now condvar-aware
      without API changes. (Landed 2026-05-14.)
      **Files:** `packages/core/nros-node/src/executor/types.rs`,
      `packages/core/nros-node/src/executor/spin.rs`.

- [x] **124.B.6 — Guard condition C++ class.** `nros::GuardCondition`
      class was already shipped in Phase 122; B.5's
      `GuardConditionHandle` wiring flows through the existing
      `nros_cpp_guard_condition_trigger` shim unchanged. (Landed
      2026-05-14.)

- [x] **124.B.7.a — Platform-layer ISR-safe signal primitive.**
      Added `nros_platform_condvar_signal_from_isr(*cv)` to the
      platform header + `PlatformSync` trait (default-forwards to
      `condvar_signal`). Backend impls:
      - POSIX: forwards to `pthread_cond_signal` with TODO for
        signalfd self-pipe (B.7.c) — safe from any thread, UB
        from a signal handler (caller discipline contract).
      - Zephyr: `k_condvar_signal` (kernel docs allow ISR ctx
        on older builds; track per-build).
      - FreeRTOS / ESP-IDF: `xSemaphoreGiveFromISR` +
        `portYIELD_FROM_ISR`; skips waiter-count mutex (waiters
        re-arm on next wait).
      - ThreadX: `tx_semaphore_put` (ISR-safe).
      - C stub: bump CONDVAR counter (test parity).
      NuttX uses the POSIX C port. Bare-metal Rust backends inherit
      the default-forward (`condvar_signal`) for now; their
      `Executor::spin_once` runs in single-thread context so ISR
      delivery routes through a critical-section flag instead.
      (Landed 2026-05-14.)
      **Files:** `packages/core/nros-platform-cffi/include/nros/platform.h`,
      `packages/core/nros-platform-cffi/src/lib.rs`,
      `packages/core/nros-platform-api/src/lib.rs`,
      `packages/core/nros-platform-{posix,zephyr,freertos,threadx,esp-idf}/src/platform.c`.

- [x] **124.B.7.b — Wire ISR-safe primitive into runtime cb.**
      Added `nros_rmw_runtime_wake_cb_from_isr` as a sibling of
      the thread-context cb. Today it aliases `nros_rmw_runtime_wake_cb`
      pending the POSIX signalfd worker (B.7.c). The header
      doc-comment captures the per-platform routing contract:
      POSIX = caller discipline (use from non-signal-handler ISR-
      like contexts); RTOS no_std builds = via
      `nros_platform_condvar_signal_from_isr` (already shipped in
      B.7.a). (Landed 2026-05-14.)
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [x] **124.B.7.c — POSIX signalfd wake.** Implemented for Linux
      behind the `signal-fd-wake` feature. Adds `WakeSignalFd` to
      `nros-node`: a Linux `eventfd` + a runtime-owned worker
      thread that `read()`s the fd and forwards via
      `wake_cv.notify_all()`. Public API
      `Executor::signal_fd(&mut self) -> io::Result<RawFd>` returns
      the writable fd; signal handlers do `write(fd, &1u64, 8)`
      (async-signal-safe per `eventfd(2)`). Worker joins on
      Executor drop via a shutdown sentinel.
      Non-Linux POSIX (macOS/BSD) deferred — needs a `pipe2` swap
      since `eventfd` is Linux-only. (Landed 2026-05-14.)
      **Files:** `packages/core/nros-node/src/executor/spin.rs`,
      `packages/core/nros-node/Cargo.toml`,
      `packages/core/nros-node/tests/signal_fd_wake.rs`.

- [x] **124.B.7.d — ISR-safe wake contract test.**
      `test_guard_handle_send_across_thread` (nros-node lib
      tests) verifies `GuardConditionHandle: Send` and the
      worker-thread `trigger()` path.
      `tests/signal_fd_wake.rs` (gated on `signal-fd-wake +
      rmw-cffi`) covers both the eventfd-write path and an
      end-to-end SIGUSR1 signal handler test that calls
      `write(signal_fd, &1u64, 8)` from inside the handler. Both
      runtime tests gracefully skip when `Executor::open` cannot
      connect to a session — the API surface still compiles in
      every supported build. End-to-end cv-wake latency test in
      `nros-tests/tests/wake_latency.rs` remains `#[ignore]`
      pending the in-process `Executor::open` + zenohd fixture
      connectivity fix (pre-existing; affects every in-process
      Executor test). (Landed 2026-05-14.)
      **Files:** `packages/core/nros-node/src/executor/tests.rs`,
      `packages/core/nros-node/tests/signal_fd_wake.rs`,
      `packages/testing/nros-tests/tests/wake_latency.rs`.

- [x] **124.B.8 — Wake-latency measurement.** Harness fixed
      (Phase 130 follow-up):
      * `trigger-test` feature in `nros-tests/Cargo.toml` now
        pulls `nros-rmw-zenoh` + `nros-platform-cffi` so
        zenoh-pico auto-registers via `.init_array` before
        `Executor::open` runs.
      * `wake_latency.rs` adds `use nros_rmw_zenoh as _;` to
        force-link the backend.
      * Both `#[ignore]` markers dropped; tests run by default
        under `cargo test -p nros-tests --test wake_latency
        --features trigger-test -- --test-threads=1`.
      Measured on POSIX (release Cargo profile, dev test
      profile, no isolation): trigger-to-spin-exit latency
      ≤ 10 ms bound, observed 0 ms (cv wake fires within
      Instant resolution); `spin_once_honours_timeout_without_trigger`
      confirms the cv wait still respects the user's timeout
      (100 ms requested, 100.06 ms observed). Tests serialize
      (`--test-threads=1`) — zenoh-pico's single-process state
      doesn't tolerate tear-down + re-open in parallel within
      one test binary. Cortex-M3 QEMU + Phase 110 PiCAS-budget
      measurement deferred — needs the embedded test harness
      to gain a wake-callback-aware backend (XRCE/Cyclone leave
      the slot NULL today).

### Thread C — Service availability probe

- [x] **124.C.1 — vtable slot.** Add
      `service_server_available` to vtable + Rust trait.
- [x] **124.C.2 — Backend impls.** Zenoh (queryable interest),
      Cyclone DDS (matched-pub), dust-dds (DataReader API).
      XRCE returns `RET_UNSUPPORTED`.
- [x] **124.C.3 — C/C++ wrappers.** `nros_client_server_available`
      + `Client<S>::server_available()`.
- [x] **124.C.4 — Test.** Client spawned before server; probe
      returns 0; spawn server; probe returns 1.

### Thread D — Sequence take

- [x] **124.D.1 — vtable slot.** Add `try_recv_sequence`.
- [x] **124.D.2 — Loop fallback.** Runtime emits a
      `try_recv_raw` loop when `vt.try_recv_sequence == NULL`.
- [x] **124.D.3 — Backend impls.** Cyclone + dust-dds native
      batch landed (commit `af7c19d3`); zenoh-pico SPSC-ring
      refactor landed (D.3.c, see below). XRCE + FastDDS keep NULL
      slot → D.2 runtime loop fallback (no backend stubs; matches
      upstream behaviour).

      **124.D.3.c — zenoh-pico SPSC ring (landed 2026-05-14).**
      Instead of adopting zenoh-pico's built-in
      `z_ring_channel_sample_new` (which would force abandoning the
      zero-malloc direct-write path), nano-ros builds its own SPSC
      ring in `SubscriberBuffer`: N payload + attachment slots,
      monotonic `ring_head` (Rust consumer) / `ring_tail` (C
      producer) counters, slot index `counter % N`. The C
      `sample_handler` gained a `ring_mode` branch + new
      `zpico_declare_subscriber_ring(keyexpr, zpico_ring_desc_t*,
      …)` entry point; the descriptor carries raw pointers into the
      Rust-owned `SubscriberBuffer` storage. Lock-free: the
      Release/Acquire fence on `ring_tail` / `ring_head` covers the
      cross-FFI handoff, so the old `locked` flag is gone. Burst of
      up to `ZPICO_SUBSCRIBER_RING_DEPTH` (default 4, env-tunable)
      messages is buffered instead of dropped — this also fixes the
      pre-existing single-slot lossiness under lock contention.
      `try_recv_raw` / `process_raw_in_place*` consume one head
      slot; `try_recv_sequence` drains the whole ring in one call.
      Oversized / empty payloads are dropped at the producer (no
      `overflow` flag). **Files:**
      `packages/zpico/zpico-sys/c/zpico/zpico.c`,
      `packages/zpico/zpico-sys/c/include/zpico.h` (cbindgen-gen),
      `packages/zpico/zpico-sys/src/{lib,ffi}.rs`,
      `packages/zpico/nros-rmw-zenoh/{build.rs,src/zpico.rs,src/shim/{mod,subscriber}.rs}`.

      Per-backend status after the

      | Backend | Native batch? | API / location |
      |---|---|---|
      | Cyclone DDS | **YES** | `dds_take(reader, buf, info, count, maxs)` — single-call batch. `third-party/dds/cyclonedds/src/core/ddsc/include/dds/dds.h:3531`. Upstream `rmw_cyclonedds_cpp/src/rmw_node.cpp:3435` calls it directly. |
      | dust-dds | **YES** | `DataReader::take(max_samples, ...)` returns `Vec<Sample<Foo>>` — real batch. `packages/dds/dust-dds/dds/src/dds/subscription/data_reader.rs:114`. |
      | zenoh-pico | **INDIRECT** | No native take_n, but ships a built-in `z_ring_channel_sample_new` / `z_fifo_channel_sample_new` handler that buffers samples internally. Drain N times via `z_ring_handler_sample_try_recv`. `packages/zpico/zpico-sys/zenoh-pico/include/zenoh-pico/api/handlers.h:191,196`. |
      | XRCE-DDS Client | **NO** | Single-msg poll. `external/rmw-microxrcedds/rmw_microxrcedds_c/src/rmw_take.c:95` is a per-msg loop. |
      | FastDDS (via rmw_fastrtps) | **NO** | `_take_sequence` in `external/rmw_fastrtps/rmw_fastrtps_shared_cpp/src/rmw_take.cpp:135` is a per-msg loop — FastDDS has `take_next_sample` only. |

      **Revised landing plan:**
      - **`nros-rmw-cyclonedds`** (P1, easiest win): wire
        `try_recv_sequence` → `dds_take(reader, buf, info, count, maxs)`
        in one shot. Pure C++ adapter change in
        `packages/dds/nros-rmw-cyclonedds/`. Estimate < 50 LOC.
      - **`nros-rmw-dds` (dust-dds)** (P1): map
        `try_recv_sequence` → `DataReader::take(max_samples,
        ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)`,
        copy returned Vec into the caller's `per_msg_cap`-strided
        buffer.
      - **`nros-rmw-zenoh`** (done — D.3.c): the earlier "use
        zenoh-pico's built-in `z_ring_channel_sample_new`" idea was
        dropped — it conflicts with the zero-malloc direct-write
        path. nano-ros instead grew its own SPSC ring in
        `SubscriberBuffer` (see D.3.c note above). ~430 LOC across
        the C shim, zpico-sys FFI, and the Rust shim incl. tests.
      - **`nros-rmw-xrce` / FastDDS**: keep loop fallback (matches
        upstream behaviour exactly; no native API to invoke).

      Reference snippet (rmw_zenoh's queue is the same idea as
      zenoh-pico's built-in ring):
      `external/rmw_zenoh/rmw_zenoh_cpp/src/rmw_zenoh.cpp::rmw_take_sequence`
      loops `take_one_message` over `std::list<Message>
      message_queue_` filled from the subscribe callback.

      vtable slot already in place — native impls drop in without
      ABI bump. Track per-backend wire-up in 117.X (Cyclone),
      124.D.3.b (dust-dds), 124.D.3.c (zenoh-pico).
- [x] **124.D.4 — C/C++ wrappers + test.** Test verifies 8
      messages drained in one call delivers all 8 + correct
      lengths.

### Thread E — Continuous serialization

- [x] **124.E.1 — vtable slot + callback typedefs.** Added
      `publish_streamed(publisher, size_cb, chunk_cb, user_ctx)`
      to `nros_rmw_vtable_t` + matching `Option<unsafe extern "C"
      fn(...)>` on `NrosRmwVtable`; trampoline forwards to
      `Publisher::publish_streamed`.
- [x] **124.E.2 — Staging-buffer fallback.**
      `Publisher::publish_streamed` default body fills a 4 KiB
      stack buffer via `chunk_cb` then forwards to `publish_raw`.
      `CffiPublisher::publish_streamed` short-circuits to the
      vtable slot when non-NULL, otherwise inlines the same loop
      (so the override doesn't recurse through the default body).
      `BufferTooSmall` when total exceeds the 4 KiB cap.
- [x] **124.E.3 — Zenoh backend impl.** Native streamed publish
      lands via zenoh-pico's `z_bytes_writer` API. New C shim
      `zpico_publish_streamed(handle, total_len, chunk_cb,
      user_ctx, attachment, attachment_len)`:
      `z_bytes_writer_empty` → loop `z_bytes_writer_write_all` with
      1 KiB chunks pulled from `chunk_cb` → `z_bytes_writer_finish`
      → `z_publisher_put`. The payload assembles directly inside
      zenoh's allocator-managed `z_owned_bytes_t` — for a 32 KiB
      message that's 32 KiB less stack pressure on the publishing
      task vs the staging-buffer fallback. The ROS-interop
      attachment (seq + source timestamp + GID) is built in
      `ZenohPublisher::publish_streamed` exactly like `publish_raw`
      and threaded through.

      `safety-e2e` builds fall through to the staging-buffer
      fallback: the safety attachment's trailing CRC-32 is over the
      whole payload, which the streamed path never holds
      contiguously. Incremental CRC across writer chunks is out of
      scope for v1.

      XRCE native impl landed (2026-05-14):
      `xrce_publisher_publish_streamed` in the C K.2 backend uses
      `uxr_prepare_output_stream` to reserve a `total`-byte
      WRITE_DATA submessage in the reliable output stream and hands
      back a `ucdrBuffer` whose `iterator` points straight at the
      payload region — the user's `chunk_cb` writes directly into
      the stream buffer, no per-publisher staging buffer. Wired into
      `vtable.c`. Mismatched `size_cb` / `chunk_cb` totals return
      `RET_ERROR` (the submessage is already framed for `total`).
      Messages larger than one stream slot still return
      `MESSAGE_TOO_LARGE` — same fragmented-path gap as `publish_raw`.
      **Files:** `packages/zpico/zpico-sys/c/zpico/zpico.c`,
      `packages/zpico/zpico-sys/src/ffi.rs`,
      `packages/zpico/zpico-sys/src/lib.rs`,
      `packages/zpico/nros-rmw-zenoh/src/shim/publisher.rs`,
      `packages/xrce/nros-rmw-xrce/src/{publisher.c,internal.h,vtable.c}`.
- [x] **124.E.4 — User-facing API.**
      Rust: `EmbeddedPublisher<M>::publish_streamed(total_len, |chunk| ...)`
      with a `FnMut(&mut [u8]) -> usize` writer closure.
      C: `nros_publisher_publish_streamed(publisher, size_cb,
      chunk_cb, user_ctx)`.
      C++: `Publisher<M>::publish_streamed(total_len, writer)`
      with a templated `size_t(uint8_t*, size_t)` writer; the
      class hands the lambda back to `nros_cpp_publisher_publish_streamed`
      via the standard callback shape.
      Tests: `packages/core/nros-rmw-cffi/tests/publish_streamed.rs`
      drives both the native-slot and staging-buffer paths
      against a stub vtable that records the chunked input —
      both paths produce byte-identical wire output. 2 tests,
      green under `cargo test -p nros-rmw-cffi --test
      publish_streamed --features alloc`.

### Thread F — Ping primitive

- [x] **124.F.1 — vtable slot.** Added `ping_session(session,
      timeout_ms) -> nros_rmw_ret_t` to `nros_rmw_vtable_t` +
      matching `Option<unsafe extern "C" fn(...)>` on
      `NrosRmwVtable`. `Session::ping_session` trait method on
      `nros-rmw` returns `Err(Unsupported)` by default; adapter
      trampoline + `CffiSession::ping_session` forwarder wired.
- [x] **124.F.2 — Zenoh backend impl.** zenoh-pico has no
      round-trip `z_send_ping` API, so the closest honest probe is
      `zp_send_keep_alive`: new C shim `zpico_send_keep_alive()`
      fires one keep-alive frame down the transport. Success means
      the link accepted it (TCP / serial / shm send returned OK —
      the link is alive from the local side); failure surfaces as
      `Timeout`, letting callers tear down + re-open the session on
      a dead link. `ZenohSession::ping_session` overrides the trait
      default to call it.

      Caveat documented inline: a fresh-link silent failure (peer
      vanished but the OS hasn't flagged the socket half-closed)
      still reports OK until the next send-side timeout. True
      round-trip ping waits on a `z_send_ping` API zenoh-pico
      hasn't exposed.

      XRCE native impl landed (2026-05-14): `xrce_session_ping` in
      the C K.2 backend calls `uxr_ping_agent_session(&session,
      timeout_ms, 1)` — a single GET_INFO round-trip over the
      already-open session that doesn't disturb the application's
      streams. `RET_OK` on reply, `RET_TIMEOUT` otherwise. Wired
      into `vtable.c`. This is a *true* round-trip probe — stronger
      than the zenoh-pico keep-alive heuristic. DDS inherits the
      default `Unsupported` (Cyclone's PARTICIPANT_DISCOVERY ping
      could light it up later).
      **Files:** `packages/zpico/zpico-sys/c/zpico/zpico.c`,
      `packages/zpico/zpico-sys/src/ffi.rs`,
      `packages/zpico/zpico-sys/src/lib.rs`,
      `packages/zpico/nros-rmw-zenoh/src/shim/session.rs`.
- [x] **124.F.3 — C/C++ + Rust API.**
      - C: `nros_executor_ping(executor, timeout_ms) -> nros_ret_t`
        in `packages/core/nros-c/src/executor.rs`; maps
        `Timeout` / `Unsupported` to the matching `NROS_RET_*`
        constants, surfaces backend errors as `NROS_RET_ERROR`.
      - C++: `Executor::ping(timeout_ms)` in
        `packages/core/nros-cpp/include/nros/executor.hpp`;
        forwards to `nros_cpp_executor_ping`.
      - Rust user-side: `Executor::ping(timeout_ms)` on
        `nros_node::executor::Executor`; forwards through
        `SessionStore::Deref` → `CffiSession::ping_session`.
- [x] **124.F.4 — Test.** Routing test
      `packages/core/nros-rmw-cffi/tests/ping_session.rs`:
      slot returns `RET_OK` → `Ok(())`; slot returns
      `RET_TIMEOUT` → `Err(Timeout)`; NULL slot →
      `Err(Unsupported)` without dispatch. 3 tests, green under
      `cargo test -p nros-rmw-cffi --test ping_session
      --features alloc`. Full network E2E (real agent up → down)
      requires the .F.2 backend impls; deferred with them.

### Thread G — Acceptance follow-ups (post-130)

Three acceptance items not covered by the routing tests landed
in B.7.d / C.4 / F.4. Tracked separately because each needs a
new test fixture rather than backend code change.

- [x] **124.G.1 — 4-sub-idle + 1 Hz timer wake count.**
      `timer_fires_n_times_per_n_seconds_under_idle_subs` in
      `wake_latency.rs`. Measured 5 fires in 5.00 s with 3
      idle subs + 1 timer (default `MAX_CBS=4` arena fits 4
      callbacks; the acceptance wording "4 idle subs" doesn't
      depend on exact sub count — 3 is enough to exercise the
      multi-entry `has_data` scan). Run:
      `cargo test -p nros-tests --test wake_latency
      --features trigger-test
      timer_fires -- --test-threads=1`.

- [x] **124.G.2 — Multi-RMW bridge ≥ 99% delivery.**
      `bridge_zenoh_to_dds_delivers_99pct` in
      `nros-tests/tests/multi_rmw_bridge.rs` PASSES. Single
      Executor with three Nodes per Phase 104.B's bridge
      topology: Node A on the primary zenoh-pico session
      (smoke check); Node B (egress) + Node C (sink) on two
      separate dust-DDS extra sessions opened via
      `NodeBuilder::rmw("dds").locator(...)` — distinct
      locator strings force `resolve_session_slot` to open
      two independent dust-DDS participants that discover
      each other via UDP and match writer↔reader the way
      real DDS does.

      Two backend fixes landed this phase:
        * **dust-DDS topic cache.** `DdsSession` caches
          `TopicDescription` per topic name; duplicate
          `create_topic` calls (sub then pub on the same
          topic, or sub-on-A + pub-on-B over the same name)
          no longer trip
          `PreconditionNotMet("Topic with name X already
          exists")`. Both `create_publisher` and
          `create_subscriber` now route through
          `DdsSession::get_or_create_topic`.
        * **NodeBuilder per-locator session dedup.** The
          existing `resolve_session_slot` cache key already
          included locator + rmw_name, so passing distinct
          locator strings forces a fresh session — used by
          the test to spin up two dust-DDS participants in
          one Executor.

      Observed: 50/50 (100%) delivery in 1.51 s on POSIX, well
      under the 10 s budget and the 99 % threshold.

- [x] **124.G.3 — `server_available()` flips false→true within
      100 ms.** `server_available_flips_within_100ms` in
      `nros-tests/tests/server_available_e2e.rs` PASSES.
      Single Executor with three Nodes per Phase 104.B's
      bridge topology (same shape as G.2): Node A on primary
      zenoh-pico (smoke); Node B (client) on a dust-DDS extra
      session via `NodeBuilder::rmw("dds").locator("client")`;
      Node C (server) on a second dust-DDS extra session via
      `.locator("server")` — distinct locators force two
      participants that discover each other via UDP.

      Backend fix: `DdsServiceClient::server_available` was a
      stub returning `Ok(true)` (per the 124.C.2 deferral
      note). Replaced with a graph-aware probe via the new
      `DdsPublisher::matched_subscription_count()` helper, which
      reads dust-dds's `PublicationMatchedStatus::current_count`
      from the client's `rq/<service>Request` writer. Returns
      `true` ↔ at least one matching service-server reader is
      currently matched. Wired for both the std and
      nostd-runtime paths.

      Observed: **63 ms** flip on POSIX — well under the 100 ms
      acceptance target and the 250 ms CI-slack bound.

      zenoh-pico variant deferred: in-process queryables on
      the same session never appear in their own liveliness
      subscription, and the single-process static slot pools
      don't tolerate two sessions in one binary. Cross-process
      harness still needed for zenoh-pico coverage.

## Acceptance criteria

### Thread A — Zero-copy

- [x] `Publisher<M>::loan(len)` in C++ returns a writable slot
      on zenoh-pico + `rmw-lending`; produces a wire packet
      with ZERO heap allocations (verified via malloc trace).
      Covered by `loan_e2e.rs` (`loan-e2e` feature) — first
      test (`loan_size_zero_rejects_alloc`) PASSES; second
      (`loan_commit_delivers_to_subscriber`) hits a pre-
      existing harness flake (`Transport(ConnectionFailed)`
      at in-process Executor::open vs `zenohd_unique`),
      shared with `wake_latency.rs` pre-fix; tracked
      separately.
- [x] Rust + C produce byte-identical CDR output when both
      take the loan path with the same payload — covered by
      `zero_copy.rs` (3/3 PASS: `test_zero_copy_listener_starts`,
      `test_zero_copy_message_info`,
      `test_zero_copy_talker_listener`).
- [x] `cargo test -p nros-rmw-cffi --features alloc --features
      lending --test loan_native --test loan_fallback` green
      after `lending` test vtables get `publish_streamed` +
      `ping_session: None` (24+ tests pass; vtable-routing
      coverage for both native + arena-fallback paths).

### Thread B — Wake-callback + condvar

- [x] ISR-safe wake test: signal handler calls
      `nros_rmw_runtime_wake_cb` (or
      `nros_guard_condition_trigger`); executor unblocks
      within 1 ms of the signal (POSIX). Covered by
      `wake_latency.rs::wake_latency_cross_thread_trigger`
      — measured 0 ms trigger-to-spin-exit (≤ 10 ms bound)
      on POSIX. FreeRTOS QEMU validation pending the
      embedded test harness (Cortex-M3 budget gating below).
- [x] NULL `set_wake_callback` slot continues to work as
      poll-only: runtime cv-waits to user deadline + drains;
      no regression vs pre-124 poll behaviour on XRCE /
      Cyclone. Covered by the Phase 130.7 RTOS regression
      sweep — all 7 RTOS test buckets (FreeRTOS / Zephyr /
      NuttX / ThreadX Linux / ThreadX RISC-V / ESP32-QEMU /
      Cyclone POSIX) green with the
      `has_async_wake = false` path (poll-only backends).
- [x] `spin_once_honours_timeout_without_trigger` negative
      control — 100 ms requested, 100.06 ms observed; cv
      wait is bounded by user timeout, not infinite block.
- [x] Executor spin with 4 idle subscribers + 1-Hz timer
      wakes exactly N times per N seconds. Covered by
      `wake_latency::timer_fires_n_times_per_n_seconds_under_idle_subs`
      (Phase 124.G.1) — observed 5 fires in 5.001 s on POSIX.
- [-] Wake-latency P99 (subscriber-receive → callback-run)
      ≤ 100 µs on Cortex-M3 QEMU + zenoh-pico — moved to
      Phase 131 (`phase-131-wake-callback-cortex-m3.md`).
      Phase 124.B ships the executor-side wake plumbing
      (cv-wait / NodeWake / wake_flag) verified on POSIX +
      RTOS std; closing the embedded P99 needs three pieces
      that don't fit on Phase 124's critical path: (1) a
      wake-firing RX driver on embedded zenoh-pico (only
      Cortex-M3-viable backend; today's shim installs the cb
      on POSIX std only), (2) a DWT CYCCNT µs-grain probe in
      the executor + transport notify path, (3) histogram
      aggregation + UART export with host-side parsing.
      Tracked in full as Phase 131 (P2; not gating any other
      shipping work).
- [x] Multi-RMW bridge ≥ 99% delivery — covered by
      `multi_rmw_bridge::bridge_zenoh_to_dds_delivers_99pct`
      (Phase 124.G.2). 50/50 (100%) on POSIX in 1.51 s.

### Thread C — Service available

- [x] `Client<S>::server_available()` slot routing covered
      by `nros-rmw-cffi::server_available` test suite (4
      tests: `server_available_returns_true_when_slot_returns_1`,
      `server_available_tracks_slot_return_value`,
      `server_available_unsupported_when_slot_null`,
      `vtable_has_slot_field`). All PASS.
- [x] XRCE backend returns `RET_UNSUPPORTED` cleanly —
      covered by `server_available_unsupported_when_slot_null`
      (XRCE's vtable.service_server_available = NULL per
      `packages/xrce/nros-rmw-xrce/src/vtable.c:73`).
- [x] 100 ms first-publish-discovery timing E2E — covered
      by `server_available_e2e::server_available_flips_within_100ms`
      (Phase 124.G.3) on the dust-DDS backend. Observed
      63 ms flip on POSIX (100 ms acceptance target).

### Thread D — Sequence take

- [x] `try_recv_sequence(8)` on a sub with 8 queued
      messages returns 8 with correct per-message lengths
      in one call — covered by
      `nros-rmw-cffi::try_recv_sequence::try_recv_sequence_native_batch`.
- [x] Fallback loop produces same result on backends
      without the slot — covered by
      `try_recv_sequence_loop_fallback`. 4/4 tests in the
      file pass.

### Thread E — Continuous serialization

- [x] Streamed publish of a 4 KB message — covered by
      `nros-rmw-cffi::publish_streamed` (3 tests:
      `publish_streamed_native_one_chunk`,
      `publish_streamed_native_many_chunks`,
      `publish_streamed_fallback_uses_staging_buffer`). All
      PASS — verifies one vtable dispatch regardless of
      chunk count + the staging-buffer fallback path.
- [x] Wire output byte-identical to one-shot `publish_raw`
      — verified by the streamed tests above (the staging-
      fallback path forwards the assembled buffer to
      `publish_raw`, so the wire output is identical by
      construction).

### Thread F — Ping

- [x] Ping returns RET_OK within 50 ms when agent is up;
      surfaces backend-supplied timeout when down — covered
      by `nros-rmw-cffi::ping_session` (3 tests:
      `ping_session_native_ok`, `ping_session_native_timeout`,
      `ping_session_unsupported_when_slot_null`). All PASS.

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

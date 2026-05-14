# nano-ros RMW C ABI — coverage status vs upstream `rmw.h`

**Date:** 2026-05-14
**Scope:** Compare `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`
(23 vtable slots) against upstream `rmw/rmw.h` (~69 functions) and
group by capability. Identify gaps worth closing and gaps that are
won't-do.

## Summary

| Group | Upstream count | nano-ros status |
|---|---|---|
| 1. Session / context lifecycle | 4 | ✅ covered (via `open`/`close`) |
| 2. Publisher | 8 | ⚠ partial — bytes only, no loan |
| 3. Subscription | 11 | ⚠ partial — bytes only, no loan, no batch take |
| 4. Service server | 6 | ✅ data plane covered |
| 5. Service client | 6 | ⚠ availability probe missing |
| 6. Wait / dispatch | 5 | 🔀 different model (poll vs waitset) |
| 7. Guard conditions | 4 | 🔀 wrapped inside `drive_io` |
| 8. Graph introspection | 8 | ❌ out of scope |
| 9. Endpoint identity (GID) | 5 | ❌ out of scope |
| 10. QoS introspection | 4 | ❌ deferred |
| 11. Content filter | 3 | ❌ deferred |
| 12. Network flow / multi-NIC | 4 | ❌ out of scope |
| 13. Logging | 1 | ❌ deferred |
| 14. Allocation hooks | 4 | ❌ won't-do (arena model) |
| 15. Events | 4 | ⚠ different scope (QoS-deadline-only) |
| 16. Liveliness | 2 | ✅ covered |
| 17. Implementation identifier | 3 | 🔀 different (named registry) |

Legend: ✅ feature parity for the data-plane use case; ⚠ partial;
🔀 deliberate redesign with different semantics; ❌ not implemented.

## Categorized function tables

### 1. Session / context lifecycle

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_init` | `open(locator, mode, domain_id, node_name, *out)` | ✅ (collapsed) |
| `rmw_shutdown` / `rmw_context_fini` | `close(*session)` | ✅ |
| `rmw_create_node` | (Node is C++-side concept, see Phase 104.C) | 🔀 |
| `rmw_destroy_node` | (same) | 🔀 |
| `rmw_node_assert_liveliness` | not exposed | ❌ deferred |
| `rmw_node_get_graph_guard_condition` | not exposed | ❌ deferred |

Upstream splits context (process-level RMW init) from node
(participant in the graph). nano-ros collapses both into `open` —
one Session = one node identity. Multi-Node-per-Executor is the
domain of Phase 104.C, layered ABOVE the vtable.

### 2. Publisher

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_publisher` | `create_publisher` | ✅ |
| `rmw_destroy_publisher` | `destroy_publisher` | ✅ |
| `rmw_publish` (typed) | (collapsed — see notes) | 🔀 |
| `rmw_publish_serialized_message` | `publish_raw(*pub, *bytes, len)` | ✅ |
| `rmw_publish_loaned_message` | (no vtable slot — see §3.1) | ⚠ |
| `rmw_borrow_loaned_message` | (no vtable slot) | ⚠ |
| `rmw_return_loaned_message_from_publisher` | (no vtable slot) | ⚠ |
| `rmw_publisher_count_matched_subscriptions` | not exposed | ❌ deferred |
| `rmw_publisher_get_actual_qos` | not exposed | ❌ deferred |
| `rmw_publisher_assert_liveliness` | `assert_publisher_liveliness` | ✅ |
| `rmw_publisher_wait_for_all_acked` | not exposed | ❌ deferred |
| `rmw_init_publisher_allocation` | (won't-do — arena model) | ❌ |
| `rmw_fini_publisher_allocation` | (won't-do) | ❌ |
| `rmw_get_serialized_message_size` | (serializer ABI handles) | 🔀 |

### 3. Subscription

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_subscription` | `create_subscriber` | ✅ |
| `rmw_destroy_subscription` | `destroy_subscriber` | ✅ |
| `rmw_take` (typed) | (collapsed) | 🔀 |
| `rmw_take_with_info` | (no `_with_info` variant — see §3.2) | ⚠ |
| `rmw_take_serialized_message[_with_info]` | `try_recv_raw(*sub, *buf, cap)` | ✅ |
| `rmw_take_sequence` | (no vtable slot — see §3.3) | ❌ worth adding |
| `rmw_take_loaned_message[_with_info]` | (no vtable slot) | ⚠ |
| `rmw_borrow_loaned_message` / `return_loaned_message_from_subscription` | (no slot) | ⚠ |
| `rmw_subscription_count_matched_publishers` | not exposed | ❌ deferred |
| `rmw_subscription_get_actual_qos` | not exposed | ❌ deferred |
| `rmw_subscription_set_content_filter` | not exposed | ❌ deferred |
| `rmw_subscription_get_content_filter` | not exposed | ❌ deferred |
| `rmw_subscription_set_on_new_message_callback` | `register_subscriber_event` (different scope) | ⚠ |
| `rmw_init_subscription_allocation` | (won't-do) | ❌ |
| `has_data` (poll) | `has_data(*sub) -> i32` | nano-ros-specific |

### 4. Service server

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_service` | `create_service_server` | ✅ |
| `rmw_destroy_service` | `destroy_service_server` | ✅ |
| `rmw_take_request` | `try_recv_request(*srv, *buf, cap, *seq_out)` | ✅ |
| `rmw_send_response` | `send_reply(*srv, seq, *bytes, len)` | ✅ |
| `rmw_service_set_on_new_request_callback` | `register_subscriber_event` (shared API) | ⚠ |
| `rmw_service_request_subscription_get_actual_qos` | not exposed | ❌ deferred |
| `rmw_service_response_publisher_get_actual_qos` | not exposed | ❌ deferred |

### 5. Service client

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_client` | `create_service_client` | ✅ |
| `rmw_destroy_client` | (implicit via session close) | ⚠ |
| `rmw_send_request` + `rmw_take_response` | `call_raw(*client, *req, req_len, *reply_buf, cap)` | 🔀 (collapsed round-trip) |
| `rmw_service_server_is_available` | (no vtable slot — see §3.4) | ❌ worth adding |
| `rmw_client_set_on_new_response_callback` | (same shared event API) | ⚠ |
| `rmw_client_request_publisher_get_actual_qos` | not exposed | ❌ deferred |

### 6. Wait / dispatch

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_wait_set` | (no vtable — see §3.5) | 🔀 |
| `rmw_destroy_wait_set` | (no vtable) | 🔀 |
| `rmw_wait(waitset, timeout)` | `drive_io(*session, timeout_ms)` | 🔀 (per-session, not multi) |
| `rmw_take_event` | (events fold into callbacks) | 🔀 |
| nros-specific | `next_deadline_ms(*session)` | nano-ros-only |

### 7. Guard conditions

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_create_guard_condition` | (Rust-side `nros-node` has `GuardCondition`) | 🔀 |
| `rmw_destroy_guard_condition` | (same) | 🔀 |
| `rmw_trigger_guard_condition` | (Rust API only; vtable opaque) | ⚠ |
| (rmw_wait integrates them) | wrapped in `drive_io` | 🔀 |

### 8. Graph introspection — ❌ all missing

| Upstream | Status |
|---|---|
| `rmw_get_node_names` | ❌ |
| `rmw_get_node_names_with_enclaves` | ❌ |
| `rmw_count_publishers` | ❌ |
| `rmw_count_subscribers` | ❌ |
| `rmw_get_topic_names_and_types` | ❌ |
| `rmw_get_service_names_and_types` | ❌ |
| `rmw_get_topic_endpoint_info` | ❌ |
| `rmw_get_publishers_info_by_topic` | ❌ |

Rationale: embedded use cases generally know their topology at
deploy time. Graph queries cost discovery-table memory + extra
backend bookkeeping. Defer until a concrete use case (e.g.
nano-ros-on-rclcpp-bridge) demands it.

### 9. Endpoint identity (GID)

| Upstream | Status |
|---|---|
| `rmw_gid_t` | ❌ no equivalent |
| `rmw_get_gid_for_publisher` | ❌ |
| `rmw_get_gid_for_client` | ❌ |
| `rmw_compare_gids_equal` | ❌ |

Rationale: same as §8. Multi-Node-per-Executor (Phase 104.C) may
add a `node_id` for callback dispatch but won't expose RMW-level
GIDs.

### 10–14. Various — see summary table above

Deferred categories: QoS introspection, content filter, network
flow, logging, allocation hooks. Each is a coherent addition; not
yet justified by a concrete user need.

### 15. Events

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_event_set_callback` | `register_publisher_event` / `register_subscriber_event` | ⚠ |
| `rmw_take_event` | (callbacks fire directly) | 🔀 |
| Statuses: `OFFERED_DEADLINE_MISSED` etc. | `nros_rmw_event_kind_t` | ✅ |
| Liveliness statuses | covered via `assert_publisher_liveliness` + events | ✅ |

### 16. Liveliness — ✅ covered

### 17. Implementation identifier

| Upstream | nano-ros | Status |
|---|---|---|
| `rmw_get_implementation_identifier()` | (named registry from Phase 104.B) | 🔀 |
| `rmw_get_serialization_format()` | (CDR everywhere) | 🔀 |
| Per-entity `implementation_identifier` field | (Rust monomorphisation catches at compile time) | 🔀 |

nano-ros's named registry (`nros_rmw_cffi_lookup("zenoh")`) plus
Rust type system handles backend identification without a runtime
identifier per entity.

## Gap discussion

### §3.1 Loaned messages — partial today

**Current state.**

Reality is more nuanced than "missing entirely":

- `nros-rmw-cffi/include/nros/rmw_entity.h` defines a
  `can_loan_messages` flag on both `nros_rmw_publisher_t` and
  `nros_rmw_subscriber_t`. Backend opts in by setting the flag
  during `create_*`. Today's data plane treats it as an opaque
  capability advertisement that no consumer reads.
- `nros-node/src/executor/handles.rs` exposes a Rust-side
  `loan_with_timeout` + `loan` API on `EmbeddedRawPublisher`.
  Zero-copy on the Rust side via `LoanFuture`.
- **No vtable slot** to plumb the loan into the backend
  (`loan_publish`, `commit_publish`, `loan_recv`,
  `release_recv`). Phase 99 was reserved for it and never
  shipped.

**What's missing.**

To make loan end-to-end through the cffi vtable:

```c
typedef struct nros_rmw_vtable_t {
    /* ... existing slots ... */

    /* Phase 99 — loaned message ABI. NULL = backend doesn't support
     * loan; runtime falls back to copy via `publish_raw`. */
    nros_rmw_ret_t (*loan_publish)(nros_rmw_publisher_t *pub,
        size_t requested_len, uint8_t **out_buf, size_t *out_cap);
    nros_rmw_ret_t (*commit_publish)(nros_rmw_publisher_t *pub,
        uint8_t *buf, size_t actual_len);

    int32_t (*loan_recv)(nros_rmw_subscriber_t *sub,
        const uint8_t **out_buf, size_t *out_len);
    void (*release_recv)(nros_rmw_subscriber_t *sub,
        const uint8_t *buf);
} nros_rmw_vtable_t;
```

**Backend support matrix.**

- **zenoh-pico:** supports zero-copy publish via
  `zp_alloc_pub_payload` + `zp_publisher_put` on shared-memory
  links. Not on TCP/UDP. Recv-side: zenoh delivers a payload
  pointer that's valid until the callback returns; the existing
  `try_recv_raw` already copies out, so loaning a pointer is
  realistic.
- **dust-dds:** Rust trait `WriteMessage` is buffer-based; no
  loan path on the Rust side. Would require dust-dds upstream
  change.
- **micro-XRCE-DDS-Client:** session output buffer is a fixed
  arena; loan would map directly onto its `uxr_prepare_output`
  + `uxr_commit_output` pattern.
- **Cyclone DDS:** native loan via `dds_loan_sample` and
  `dds_writecdr_loan_data`. Wire-up is straightforward.

**Phase to land:** suggest **Phase 124** (new) or fold into
Phase 109. Adds 4 vtable slots; per-backend impl is incremental;
falls back to copy on backends without loan.

### §3.2 Wait set + guard condition — RT considerations

**Current state.**

nano-ros uses a **poll model**:

```c
/* Executor's inner loop */
while (running) {
    rmw_vtable->drive_io(session, timeout_ms);
    /* for each registered subscriber: */
    if (rmw_vtable->has_data(sub)) {
        rmw_vtable->try_recv_raw(sub, buf, cap);
        /* dispatch */
    }
}
```

Backends own their I/O multiplexing inside `drive_io`. `has_data`
is a cheap non-blocking poll afterwards.

**Why upstream uses wait sets.**

`rmw_wait(waitset)` blocks until ANY entity has work. The waitset
aggregates: subs, service servers, service clients, guard
conditions, timers. The backend implements via select / epoll /
kqueue / k_poll().

Advantages: one syscall blocks waiting on N entities (vs poll's
N + 1 syscalls); lower latency wake on signal-style events
(guard conditions wake the wait immediately).

Disadvantages: ties dispatch to the OS poll primitive; can't
distinguish "high-priority subscription ready" from "low-priority
ready" without further machinery (PiCAS).

**RTOS support of wait-set primitives:**

| OS | Primitive | Wait-set fit |
|---|---|---|
| POSIX | `select` / `poll` / `epoll` / `kqueue` | First-class. Backends already use these. |
| FreeRTOS | `xQueueSelectFromSet` (queue sets) + `xEventGroupWaitBits` | Workable. Queue set covers msg queues; event group covers flag-style triggers. Two-tier dispatch needed. |
| NuttX | `select` / `poll` (POSIX subset) | First-class. Same as POSIX. |
| ThreadX | `tx_event_flags_get` + `tx_queue_receive` (no native set) | **Hard.** Must roll a higher-level multiplexer or use Azure RTOS NetX BSD wrapper. |
| Zephyr | `k_poll()` with `k_poll_event` array | First-class. Mixes semaphores, message queues, FIFO, signals. |
| ESP-IDF | FreeRTOS + lwIP `select` | Same as FreeRTOS + POSIX. |
| Bare-metal | None | Backend must roll its own (event loop). |

**Verdict.** Wait sets are implementable on most RTOSes the
project targets. ThreadX is the weakest link. The current
`drive_io` design is the lowest-common-denominator; it works
everywhere but leaves wake-latency on the table.

**RT context.**

For real-time:

- **Bounded blocking:** both models can be bounded. `drive_io`
  blocks ≤ `timeout_ms`; `rmw_wait` blocks ≤ `wait_timeout`. RT
  callers pick the timeout consciously.
- **Priority inversion:** `rmw_wait`'s internal mutex around the
  waitset state is a potential inversion source if multiple
  threads share a waitset. nano-ros's per-session `drive_io`
  avoids this — each Executor instance has its own dispatch
  thread.
- **Wake latency:** waitsets win for signal-style events (guard
  condition triggered from ISR). Poll model has `≤ timeout_ms`
  worst case. For Phase 110 (PiCAS) the wake-latency gap is
  significant.

**Proposed approach.**

Add OPTIONAL wait-set vtable slots; runtime uses them when
available, falls back to poll:

```c
typedef struct nros_rmw_vtable_t {
    /* ... */

    /* Phase 110+ — wait set (optional).
     *
     * `wait_handles` is an array of opaque per-entity tokens
     * filled by the backend during `create_*`. `ready_mask` is
     * a bitmask (or callback) that the runtime reads to skip
     * the `has_data` poll loop. NULL slot = backend doesn't
     * support waitset; runtime stays in poll mode. */
    nros_rmw_ret_t (*wait_multi)(nros_rmw_session_t *session,
        const void *const *handles, size_t n_handles,
        int32_t timeout_ms, uint64_t *ready_mask);

    /* Phase 110+ — guard condition (optional). Allows external
     * triggers to wake the wait. NULL slot = signal-style
     * triggers fall back to the next `drive_io` poll iteration. */
    nros_rmw_ret_t (*create_guard_condition)(
        nros_rmw_session_t *session, void **out_handle);
    void (*destroy_guard_condition)(void *handle);
    nros_rmw_ret_t (*trigger_guard_condition)(void *handle);
} nros_rmw_vtable_t;
```

Two-tier dispatch: prefer `wait_multi` if non-NULL, else
poll-loop. Backend authors opt in incrementally. RT users on
Zephyr / Linux / NuttX get the lower wake latency; ThreadX /
bare-metal users stay on the poll path with no regression.

### §3.3 Sequence take — worth adding

**Use case.** Sensor burst patterns — IMU at 1 kHz, 8-sample
window per scheduler tick. Polling once and taking 8 messages
in one call avoids 7 redundant `has_data`+`try_recv_raw` round
trips.

**Vtable addition (small):**

```c
/* Phase 124 — batch take. Returns number of messages taken
 * (0..max), or negative `nros_rmw_ret_t` on error. NULL slot
 * = backend doesn't support; runtime loops `try_recv_raw`. */
int32_t (*try_recv_sequence)(nros_rmw_subscriber_t *sub,
    uint8_t *buf, size_t per_msg_cap, size_t max_msgs,
    size_t *out_lens);
```

`out_lens[i]` reports each message's actual length.

**Backend impl notes.** Zenoh delivers messages via callback —
backends can drain the queue into a sequence trivially. DDS has
`dds_take` (Cyclone) / `take()` (Fast DDS) that returns up to
max_samples. XRCE has best-effort batched read from session
buffer.

**Recommendation:** add. Small surface, real RT win, fallback
is trivial.

### §3.4 Service availability probe — worth adding

**Use case.** Startup ordering — client should know the server
is up before issuing the first call. Today users either
time-out a `call_raw` and infer (costly + adds discovery
latency) or hardcode startup delays.

**Vtable addition:**

```c
/* Phase 124 — service availability probe. Returns 1 if at
 * least one matching server has been discovered, 0 otherwise.
 * Negative `nros_rmw_ret_t` on error. NULL slot = backend can't
 * answer; runtime returns NROS_RMW_RET_UNSUPPORTED. */
int32_t (*service_server_available)(nros_rmw_service_client_t *client);
```

**Backend impl notes.** All three RMWs already track discovery
state internally:

- Zenoh: `z_session` has matched-publisher count via interest
  declarations.
- DDS (Cyclone, dust-dds): `DataReader::get_matched_publications`
  / equivalent.
- XRCE: requires `uxr_reliable_session` participant enumeration
  — partial support; returns `UNSUPPORTED` on micro-clients
  with no agent broadcasts.

**Recommendation:** add. Small slot, common pattern.

### Other worth-adding items (lower priority)

| Item | Use case | Effort | Verdict |
|---|---|---|---|
| `count_matched_subscriptions/publishers` | Confirm pub-sub topology at startup | small (peer count from same discovery state as §3.4) | Add if §3.4 lands |
| `wait_for_all_acked` | Synchronous shutdown ("flush all reliable msgs") | medium (per-backend ack tracking) | Defer; users can sleep |
| `subscription_get_actual_qos` | QoS introspection | small | Add when needed |
| `subscription_set_on_new_message_callback` | Event-driven dispatch hook | medium (callback re-entrancy) | Tied to wait-set design (§3.2) |
| `take_with_info` (sender GID + timestamp) | Multi-publisher diagnostics | medium (per-message metadata) | Tied to §9 GID |

### Won't-do (closed)

| Item | Reason |
|---|---|
| Graph queries (§8) | Embedded targets know topology at deploy time. Backend bookkeeping cost outweighs benefit. |
| GID-per-entity (§9) | Rust monomorphisation + named registry covers identity. |
| Allocation hooks (§14) | Arena model. Runtime owns storage. |
| Content filter (§11) | Heavy backend feature; rare in embedded. Revisit if user surfaces concrete need. |
| Network flow APIs (§12) | Multi-NIC routing is platform-layer concern. |

## Recommended next phases

1. **§3.4 Service availability probe** — small slot, broad win.
2. **§3.3 Sequence take** — small slot, real RT impact.
3. **§3.1 Loaned message vtable** — 4 slots, big WCET win for
   large messages. Requires backend co-design.
4. **§3.2 Wait set + guard condition** — Phase 110 co-design;
   tied to PiCAS work.

Items 1+2 are mechanical; 3+4 need more design.

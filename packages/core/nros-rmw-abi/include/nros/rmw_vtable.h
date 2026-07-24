#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

#include <stdint.h>
#include <stddef.h>

#include "nros/rmw_ret.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file rmw_vtable.h
 * @brief C function table for plugging third-party RMW backends into nros.
 *
 * Implement the functions in nros_rmw_vtable_t and call
 * nros_rmw_cffi_register() before creating any nros sessions.
 *
 * **Storage ownership.** The runtime owns the entity-struct storage
 * (`nros_rmw_session_t`, `nros_rmw_publisher_t`, `nros_rmw_subscriber_t`,
 * `nros_rmw_service_server_t`, `nros_rmw_service_client_t`). Each
 * `create_*` call receives a runtime-allocated, zero-initialised struct
 * via the `out` pointer; the backend writes its `backend_data` (and
 * `can_loan_messages` for pub/sub) into it. The runtime fills the metadata
 * fields (`topic_name`, `type_name`, `qos`) before calling
 * `create_*`; the backend reads them through the same struct.
 *
 * `destroy_*` releases the backend's `backend_data` only. The struct
 * shell stays valid until the runtime drops its owner.
 *
 * **Return-value conventions.**
 *  - `open` / `close` / `drive_io` / `create_*` / `publish_raw` /
 *    `send_reply`: `NROS_RMW_RET_OK` on success, negative
 *    `nros_rmw_ret_t` constant on error (see `<nros/rmw_ret.h>`).
 *  - `try_recv_raw` / `try_recv_request` / `call_raw`: non-negative =
 *    bytes produced, negative = `nros_rmw_ret_t` error.
 *  - `has_data` / `has_request`: 1 = yes, 0 = no.
 *  - `destroy_*`: void (best-effort cleanup).
 */

typedef struct nros_rmw_vtable_t {
    /* ---- Session lifecycle ---- */
    /** Open a session. The runtime supplies a zero-initialised
     *  `nros_rmw_session_t` via @p out with `node_name` /
     *  `namespace_` already filled. The backend writes
     *  `out->backend_data`. */
    nros_rmw_ret_t (*open)(const char *locator, uint8_t mode,
                           uint32_t domain_id, const char *node_name,
                           nros_rmw_session_t *out);
    nros_rmw_ret_t (*close)(nros_rmw_session_t *session);
    nros_rmw_ret_t (*drive_io)(nros_rmw_session_t *session, int32_t timeout_ms);

    /* ---- Publisher ---- */
    /** Create a publisher. The runtime fills `out->topic_name`,
     *  `out->type_name`, `out->qos` before this call; the backend
     *  writes `out->backend_data` and `out->can_loan_messages`. */
    nros_rmw_ret_t (*create_publisher)(nros_rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_publisher_t *out);
    void (*destroy_publisher)(nros_rmw_publisher_t *publisher);
    nros_rmw_ret_t (*publish_raw)(nros_rmw_publisher_t *publisher,
        const uint8_t *data, size_t len);

    /* ---- Subscriber ---- */
    nros_rmw_ret_t (*create_subscriber)(nros_rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_subscriber_t *out);
    void (*destroy_subscriber)(nros_rmw_subscriber_t *subscriber);
    int32_t (*try_recv_raw)(nros_rmw_subscriber_t *subscriber,
        uint8_t *buf, size_t buf_len);
    int32_t (*has_data)(nros_rmw_subscriber_t *subscriber);

    /* ---- Service Server ---- */
    /* Phase 193.1b — `qos` applies to both the request + reply endpoints
       (one profile per service, mirrors create_publisher/subscriber). */
    nros_rmw_ret_t (*create_service_server)(nros_rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_service_server_t *out);
    void (*destroy_service_server)(nros_rmw_service_server_t *server);
    int32_t (*try_recv_request)(nros_rmw_service_server_t *server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out);
    int32_t (*has_request)(nros_rmw_service_server_t *server);
    nros_rmw_ret_t (*send_reply)(nros_rmw_service_server_t *server,
        int64_t seq, const uint8_t *data, size_t len);

    /* ---- Service Client ---- */
    nros_rmw_ret_t (*create_service_client)(nros_rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_service_client_t *out);
    void (*destroy_service_client)(nros_rmw_service_client_t *client);
    int32_t (*call_raw)(nros_rmw_service_client_t *client,
        const uint8_t *request, size_t req_len,
        uint8_t *reply_buf, size_t reply_buf_len);

    /** Phase 130.4 — non-blocking send_request_raw.
     *
     *  Sends the request to the backend without blocking for a
     *  reply. Returns immediately. NULL = the runtime falls back to
     *  storing the pending request in CffiServiceClient and
     *  invoking `call_raw` on the next `try_recv_reply_raw_slot`
     *  call (blocking inside the executor, the Phase 127.C.4 root
     *  cause behaviour). Backends that implement this slot must
     *  also implement `try_recv_reply_raw_slot` so the executor's
     *  poll loop can drain the reply non-blockingly. */
    nros_rmw_ret_t (*send_request_raw)(nros_rmw_service_client_t *client,
        const uint8_t *request, size_t req_len);

    /** Phase 130.4 — non-blocking try_recv_reply_raw.
     *
     *  Polls the backend for a reply. `>= 0` = reply bytes copied
     *  into `reply_buf`. `NROS_RMW_RET_NO_DATA` = no reply yet.
     *  Other negative = backend error. NULL = the runtime falls
     *  back to the blocking `call_raw` path (Phase 127.C.4
     *  behaviour). Paired with `send_request_raw` — backends
     *  implement both or neither. */
    int32_t (*try_recv_reply_raw)(nros_rmw_service_client_t *client,
        uint8_t *reply_buf, size_t reply_buf_len);

    /* ---- Phase 108 — status events (optional) ---- */
    /** Register a callback for a subscriber-side event. NULL function
     *  pointer = backend doesn't generate any subscriber events.
     *  Specific kind unsupported on a backend that supports some
     *  events = `NROS_RMW_RET_UNSUPPORTED` return.
     *  `deadline_ms` is consulted for `REQUESTED_DEADLINE_MISSED`
     *  only; ignored otherwise. */
    nros_rmw_ret_t (*register_subscriber_event)(
        nros_rmw_subscriber_t *subscriber,
        nros_rmw_event_kind_t  kind,
        uint32_t               deadline_ms,
        nros_rmw_event_callback_t cb,
        void                  *user_context);

    /** Register a callback for a publisher-side event. Same NULL /
     *  unsupported-kind conventions as `register_subscriber_event`.
     *  `deadline_ms` is consulted for `OFFERED_DEADLINE_MISSED` only. */
    nros_rmw_ret_t (*register_publisher_event)(
        nros_rmw_publisher_t  *publisher,
        nros_rmw_event_kind_t  kind,
        uint32_t               deadline_ms,
        nros_rmw_event_callback_t cb,
        void                  *user_context);

    /** Phase 108.B — manually assert this publisher's liveliness.
     *  Required for `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE` liveliness
     *  kinds; no-op (return `NROS_RMW_RET_OK`) for other kinds.
     *  NULL function pointer = backend doesn't support manual
     *  liveliness; runtime returns `NROS_RMW_RET_OK` for AUTOMATIC /
     *  NONE callers and `NROS_RMW_RET_UNSUPPORTED` for MANUAL_*. */
    nros_rmw_ret_t (*assert_publisher_liveliness)(
        nros_rmw_publisher_t *publisher);

    /** Phase 110.0 — backend's next internal-event deadline in
     *  milliseconds from now (lease keepalive, heartbeat, reader
     *  ACK-NACK timeout, etc.). The runtime caps its `drive_io`
     *  timeout against `min(user_timeout, timer_deadline, this)` so
     *  quiet links don't wake early, see no user-visible work, and
     *  round-trip back into `drive_io`.
     *
     *  Returns a non-negative milliseconds value, or a negative value
     *  meaning "no internal deadline" (treat as `None`).
     *
     *  NULL function pointer is permitted — the runtime treats it the
     *  same as a negative return. */
    int32_t (*next_deadline_ms)(const nros_rmw_session_t *session);

    /** Phase 124.B.1 — executor wake callback.
     *
     *  The runtime calls this once per session right after `open`
     *  with `cb` pointing at a runtime-supplied function and `ctx`
     *  pointing at the executor's wake state. The backend stores
     *  both in its per-session state and calls `cb(ctx)` whenever
     *  its transport-notification path fires — datagram arrival,
     *  condvar wake-up, select-fd ready, etc. The runtime cb does
     *  flag-write + condvar-signal atomically so a `spin_once`
     *  blocked on the wake condvar resumes immediately.
     *
     *  `cb == NULL` clears any previously installed callback; the
     *  backend must drop the stored (cb, ctx) and never invoke
     *  again after this returns.
     *
     *  NULL slot = backend has no asynchronous wake path (purely
     *  poll-driven: XRCE, bare-metal). The runtime still drains the
     *  session on its deadline-bound cv-wait boundary. */
    nros_rmw_ret_t (*set_wake_callback)(nros_rmw_session_t *session,
                                         void (*cb)(void *ctx),
                                         void *ctx);

    /** Phase 124.A — zero-copy publisher loan.
     *
     *  Reserve a writable slot of at least `requested_len` bytes inside
     *  the backend's outbound buffer. Returns:
     *    * `NROS_RMW_RET_OK` + writes `*out_buf` / `*out_cap` / `*out_token`.
     *    * `NROS_RMW_RET_TRY_AGAIN` if the backend has no slot
     *      available (caller may retry or fall back to a copy path).
     *    * `NROS_RMW_RET_INVALID_ARGUMENT` on bad pointers / size.
     *
     *  `*out_cap` may exceed `requested_len`. The slot's bytes are
     *  valid until the matching `pub_commit` or `pub_discard` runs.
     *  `*out_token` is an opaque per-loan handle the backend uses to
     *  match commit / discard back to the right slot.
     *
     *  NULL function pointer = backend doesn't natively lend; the
     *  runtime falls back to a per-publisher staging arena and emits
     *  a single memcpy on commit. */
    nros_rmw_ret_t (*pub_loan)(nros_rmw_publisher_t *publisher,
                                size_t                requested_len,
                                uint8_t             **out_buf,
                                size_t               *out_cap,
                                void                **out_token);

    /** Phase 124.A — commit a previously loaned slot.
     *
     *  `token` MUST be a value returned from a prior `pub_loan` on the
     *  same publisher. `actual_len` is the byte count actually
     *  written into the slot (≤ the loan's `out_cap`). Triggers the
     *  wire send.
     *
     *  NULL = paired NULL with `pub_loan`. */
    nros_rmw_ret_t (*pub_commit)(nros_rmw_publisher_t *publisher,
                                  void                 *token,
                                  size_t                actual_len);

    /** Phase 124.A — abandon a previously loaned slot.
     *
     *  Releases the slot without sending. `token` MUST be a value
     *  returned from a prior `pub_loan` on the same publisher.
     *
     *  NULL = paired NULL with `pub_loan`. */
    void (*pub_discard)(nros_rmw_publisher_t *publisher, void *token);

    /** Phase 124.A — zero-copy subscriber borrow.
     *
     *  Borrow a read-only view of the next available message in
     *  place, without copying into a caller buffer. Returns:
     *    * `>= 0` — message length; writes `*out_buf` / `*out_token`.
     *    * `0` — no message ready (subscriber empty).
     *    * `< 0` — error (see `nros_rmw_ret_t` codes negated).
     *
     *  The view is valid until the matching `sub_release` runs.
     *  Only one borrow may be outstanding per subscriber at a time —
     *  callers MUST release before requesting another borrow.
     *
     *  NULL function pointer = backend doesn't natively borrow; the
     *  runtime falls back to `try_recv_raw` into a staging buffer. */
    int32_t (*sub_borrow)(nros_rmw_subscriber_t *subscriber,
                           const uint8_t        **out_buf,
                           size_t                *out_len,
                           void                 **out_token);

    /** Phase 124.A — release a previously borrowed view.
     *
     *  `token` MUST be a value returned from a prior `sub_borrow`
     *  on the same subscriber. Lets the next message advance into
     *  the buffer.
     *
     *  NULL = paired NULL with `sub_borrow`. */
    void (*sub_release)(nros_rmw_subscriber_t *subscriber, void *token);

    /** Phase 124.C.1 — service-server availability probe.
     *
     *  Returns `1` if ≥ 1 matching server has been discovered on the
     *  RMW graph, `0` if none yet, or a negative `nros_rmw_ret_t`
     *  constant on backend error. The runtime exposes this to user
     *  code as `nros_client_server_available()` /
     *  `Client<S>::server_available()` — clients use it to gate the
     *  first `call_raw` so a startup-ordering race doesn't surface as
     *  a request-side timeout.
     *
     *  Implementation notes per backend:
     *  - **Zenoh**: `z_session` tracks matched queryables via
     *    interest declarations.
     *  - **Cyclone DDS / dust-DDS**: built-in topic readers expose
     *    matched-pub counts.
     *  - **XRCE**: agent has no participant enumeration; return
     *    `NROS_RMW_RET_UNSUPPORTED`.
     *
     *  NULL function pointer = backend cannot answer; the runtime
     *  surfaces `NROS_RMW_RET_UNSUPPORTED` to the caller. */
    int32_t (*service_server_available)(
        nros_rmw_service_client_t *client);

    /** Phase 124.D.1 — burst-take.
     *
     *  Drains up to `max_msgs` queued messages into a contiguous
     *  caller buffer in a single backend call, avoiding N × vtable
     *  dispatch when a burst-sensor subscriber catches up on a
     *  backlog (e.g. a 100 Hz IMU feed polled at 10 Hz).
     *
     *  Storage contract:
     *    * `buf` is a contiguous `max_msgs * per_msg_cap` block.
     *    * The i-th delivered message lives at `buf + i * per_msg_cap`
     *      and has byte length `out_lens[i]`.
     *    * `out_lens` is at least `max_msgs` entries long.
     *
     *  Returns:
     *    * `>= 0` — count of messages taken (0..=max_msgs).
     *    * `< 0` — `nros_rmw_ret_t` error code; partial drains MUST
     *      use the count form, not error-out.
     *
     *  NULL function pointer = backend doesn't natively batch; the
     *  runtime emits a `try_recv_raw` loop fallback in
     *  `CffiSubscriber::try_recv_sequence`. The fallback gives
     *  identical observable behaviour (each call still costs N
     *  vtable hops) but lets user code commit to the batched API. */
    int32_t (*try_recv_sequence)(nros_rmw_subscriber_t *subscriber,
                                  uint8_t              *buf,
                                  size_t                per_msg_cap,
                                  size_t                max_msgs,
                                  size_t               *out_lens);

    /** Phase 124.E.1 — streamed publish.
     *
     *  Caller hands the backend two callbacks. The backend invokes
     *  `size_cb` once to learn the total payload length, allocates
     *  a single slot of that size in its outbound buffer, then
     *  invokes `chunk_cb` repeatedly to fill the slot in chunks
     *  until the buffer is full. Saves the per-publisher staging
     *  buffer on RAM-constrained nodes — useful for large messages
     *  on MCUs where the staging buffer dominates `.bss`.
     *
     *  Callback contract:
     *    * `size_cb(*out_total_len, user_ctx)` — write the exact
     *      total payload length, in bytes, to `*out_total_len`.
     *      Called exactly once per `publish_streamed` invocation.
     *    * `chunk_cb(out_buf, cap, *out_written, user_ctx)` —
     *      write up to `cap` bytes starting at `out_buf`, then
     *      report the count written via `*out_written`. The backend
     *      may call `chunk_cb` repeatedly until the total promised
     *      by `size_cb` has been delivered. `*out_written == 0`
     *      means EOF; the backend tears down the slot.
     *
     *  Lesson from micro-ROS's
     *  `rmw_uros_set_continous_serialization_callbacks`: pass the
     *  callbacks per-call rather than binding them to publisher
     *  state, so different messages on the same publisher can use
     *  different serialisation strategies.
     *
     *  NULL function pointer = backend doesn't stream; the runtime
     *  falls back to a one-shot staging buffer (capped at the
     *  configured `NROS_MAX_STREAM_CHUNK`) + `publish_raw`. */
    nros_rmw_ret_t (*publish_streamed)(
        nros_rmw_publisher_t *publisher,
        void (*size_cb)(size_t *out_total_len, void *user_ctx),
        void (*chunk_cb)(uint8_t *out_buf, size_t cap,
                         size_t *out_written, void *user_ctx),
        void *user_ctx);

    /** Phase 124.F.1 — session-level connectivity probe.
     *
     *  Sends a wire-level round-trip probe ("is the peer / agent /
     *  router still reachable?") and waits up to `timeout_ms` for
     *  a reply. No discovery state required — cheaper than the
     *  service-availability probe (which needs matched-publication
     *  bookkeeping). Lesson from micro-ROS's
     *  `rmw_uros_ping_agent`.
     *
     *  Returns:
     *    * `NROS_RMW_RET_OK` — peer responded within budget.
     *    * `NROS_RMW_RET_TIMEOUT` — no reply before `timeout_ms`.
     *    * `NROS_RMW_RET_UNSUPPORTED` — backend can't probe (DDS
     *      with no participant introspection).
     *    * other negative — backend error.
     *
     *  Implementation notes per backend:
     *  - **Zenoh**: `z_send_ping` (or session keep-alive piggyback).
     *  - **XRCE**: `uxr_ping_agent_session_until_timeout`.
     *  - **DDS**: built-in participant ping if available, else
     *    `RET_UNSUPPORTED`.
     *
     *  NULL function pointer = runtime surfaces
     *  `NROS_RMW_RET_UNSUPPORTED` to the caller. */
    nros_rmw_ret_t (*ping_session)(
        nros_rmw_session_t *session,
        int32_t             timeout_ms);

    /* ---- Phase 231 (RFC-0038) — zero-copy in-place subscription take ---- */

    /** Capability query: does this subscriber support process_raw_in_place()?
     *  Returns 1 if yes, 0 if no. The runtime consults this at subscription
     *  registration to choose in-place dispatch over the buffered (copying)
     *  path. NULL function pointer = treated as unsupported (buffered path). */
    int32_t (*subscriber_supports_in_place)(
        nros_rmw_subscriber_t *subscriber);

    /** Borrow one ready message in place: hand its raw CDR bytes to `cb` (with
     *  the opaque `ctx`) for the duration of the call, then release the slot —
     *  no copy into a caller buffer. Returns 1 if a message was processed (`cb`
     *  invoked), NROS_RMW_RET_NO_DATA if none was ready, or a negative error.
     *  `cb` MUST NOT re-enter this subscriber's receive. NULL function
     *  pointer = unsupported (the runtime uses the buffered path). */
    int32_t (*process_raw_in_place)(
        nros_rmw_subscriber_t *subscriber,
        void                  *ctx,
        void                 (*cb)(void *ctx, const uint8_t *ptr, size_t len));
} nros_rmw_vtable_t;

/** Register a custom RMW backend under the implicit name "default".
 *  Legacy single-arg form retained for source compatibility with
 *  backend ctors authored before the named registry (Phase 104.B.2).
 *
 *  Deprecated (Phase 128.B.5): every in-tree backend now calls
 *  `nros_rmw_cffi_register_named` with its canonical name. The
 *  unnamed shim will be removed in a follow-up phase.
 *  Returns NROS_RMW_RET_OK. */
nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

/** Phase 104.B.2 — register a backend under a stable name. Multiple
 *  backends can coexist (bridge nodes); consumers select via
 *  `nros_rmw_cffi_lookup` or the higher-level
 *  `Executor::node_builder(...).rmw(...)` path.
 *
 *  Names: UTF-8, NUL-terminated, ≤ 31 bytes (excluding NUL).
 *  Reserved: "zenoh", "dds", "xrce", "cyclonedds", future "uorb".
 *  "default" is the implicit name used by `nros_rmw_cffi_register`.
 *
 *  Duplicate registration of the same name overwrites the previous
 *  vtable (idempotent for ctor-fires-twice).
 *
 *  Returns:
 *    * NROS_RMW_RET_OK on success.
 *    * NROS_RMW_RET_INVALID_ARGUMENT if name or vtable is NULL,
 *      the name is empty, or exceeds 31 bytes.
 *    * NROS_RMW_RET_ERROR if the registry is full
 *      (NROS_RMW_MAX_BACKENDS reached). */
nros_rmw_ret_t nros_rmw_cffi_register_named(const char *name,
                                            const nros_rmw_vtable_t *vtable);

/** Look up a backend's vtable by name. Returns NULL if no backend is
 *  registered under `name`. The returned pointer is valid for the
 *  program's lifetime. */
const nros_rmw_vtable_t *nros_rmw_cffi_lookup(const char *name);

/** Diagnostic helper — fills `buf` with pointers to up to `cap`
 *  registered backend names. Returns the total number of registered
 *  backends (may exceed `cap`; caller can re-query with a larger
 *  buffer). Pointer-valid for the program's lifetime. Pass
 *  `buf=NULL, cap=0` to query the count only. */
size_t nros_rmw_cffi_registered_names(const char **buf, size_t cap);


/** Phase 249 P4b.2 — convenience macro for static-library backends.
 *  Place in exactly one TU per backend to self-register the backend
 *  on library load. `REGISTER_FN` is a no-arg function that calls
 *  `nros_rmw_cffi_register_named` for the backend.
 *
 *  Example:
 *      static void zenoh_register(void) {
 *          nros_rmw_cffi_register_named("zenoh", &VTABLE);
 *      }
 *      NROS_RMW_REGISTER_BACKEND(zenoh_register)
 *
 *  HOSTED (Rust + C/C++ on a hosted loader): the macro expands to an
 *  `.init_array` constructor (`__attribute__((constructor))`) that the
 *  loader fires before `main()` — hence before `nros_support_init` /
 *  `nros::init` — calling `REGISTER_FN`. The `--whole-archive` link
 *  keeps this object's `.init_array` slot. `nros_rmw_cffi_register_named`
 *  is idempotent (same-name overwrite), so re-registration is harmless.
 *  This consolidates the former `linkme` section walk onto the ctor
 *  (RFC-0042 §D3.3; the `linkme` distributed slice / section walker is
 *  deleted).
 *
 *  EMBEDDED (bare-metal / RTOS: Zephyr, NuttX, esp-idf, VxWorks): the
 *  loader does not run `.init_array` constructors on the startup path,
 *  so the macro expands to nothing. Registration is wired explicitly
 *  by the board / typed carrier via an explicit `nros_rmw_<x>_register()`
 *  call (phase-249 P1). For C/C++-via-cmake the `nano_ros_link_rmw`
 *  strong stub is the primary registration trigger (P2b/P4a); this
 *  hosted ctor is belt-and-suspenders.
 *
 *  Gating mirrors the cyclonedds vtable.cpp constructor (off the RTOS
 *  targets) and requires GCC/Clang constructor-attribute support. */
#if (defined(__GNUC__) || defined(__clang__)) && !defined(__ZEPHYR__) &&       \
    !defined(__NuttX__) && !defined(ESP_PLATFORM) && !defined(__VXWORKS__)
#define NROS_RMW_REGISTER_BACKEND(REGISTER_FN)                                 \
    __attribute__((constructor)) static void nros_rmw_ctor_##REGISTER_FN(      \
        void) {                                                                \
        (void) REGISTER_FN();                                                  \
    }
#else
/* Embedded / unsupported toolchain: board calls nros_rmw_<x>_register(). */
#define NROS_RMW_REGISTER_BACKEND(REGISTER_FN)
#endif

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_VTABLE_H */

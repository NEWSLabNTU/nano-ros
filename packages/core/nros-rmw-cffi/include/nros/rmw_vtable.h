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
    nros_rmw_ret_t (*create_service_server)(nros_rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id,
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
        uint32_t domain_id,
        nros_rmw_service_client_t *out);
    void (*destroy_service_client)(nros_rmw_service_client_t *client);
    int32_t (*call_raw)(nros_rmw_service_client_t *client,
        const uint8_t *request, size_t req_len,
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
} nros_rmw_vtable_t;

/** Register a custom RMW backend under the implicit name "default".
 *  Legacy single-arg form retained for source compatibility with
 *  backend ctors authored before the named registry (Phase 104.B.2).
 *  New backends should call `nros_rmw_cffi_register_named` instead.
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

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_VTABLE_H */

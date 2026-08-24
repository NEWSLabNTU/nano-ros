#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

/* Phase 376 W3.a — this header defines the GENERIC RMW ABI: its types are
 * `rmw_publisher_t`, `rmw_ret_t`, and so on, with no vendor prefix, because a
 * backend author implements RMW rather than nano-ros.
 *
 * That makes it impossible to share a translation unit with upstream's
 * `<rmw/rmw.h>`, which defines the same names differently. Refuse LOUDLY rather
 * than let one definition silently win a redefinition race — two types of one
 * name whose layouts disagree is the class this repo has already paid for three
 * times in hand-mirrored FFI structs.
 *
 * A target image never links real rmw, and host-side consumers reach the
 * backend through Rust, so nothing in this repo trips this today. It exists for
 * the day something does. */
#if defined(RMW_RMW_H_) || defined(RMW__RMW_H_)
#error "nros/rmw_vtable.h defines the generic RMW ABI and cannot share a translation unit with upstream <rmw/rmw.h>. Include one or the other."
#endif

#include <stdbool.h>
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
 * (`rmw_session_t`, `rmw_publisher_t`, `rmw_subscription_t`,
 * `rmw_service_t`, `rmw_client_t`). Each
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
 *  - `create_session` / `destroy_session` / `drive_io` / `create_*` / `publish_raw` /
 *    `send_reply`: `NROS_RMW_RET_OK` on success, negative
 *    `rmw_ret_t` constant on error (see `<nros/rmw_ret.h>`).
 *  - `try_recv_raw` / `try_recv_request` / `try_recv_reply_raw`: non-negative =
 *    bytes produced, negative = `rmw_ret_t` error.
 *  - `has_data` / `has_request`: 1 = yes, 0 = no.
 *  - `destroy_*`: void (best-effort cleanup).
 */


/* ---- Phase 376 W4 — graph enumeration visitors ----
 *
 * Upstream returns `rcutils_string_array_t` and `rmw_names_and_types_t`, which
 * ALLOCATE — two levels deep for names-and-types. There is no allocator at this
 * seam, and a caller-provides-the-buffer shape is worse than it looks: the ROS
 * graph has no bound the CALLER can know, so a 128 KiB image would have to
 * reserve permanently for the worst graph it might ever meet, and the backend
 * would still have to materialise the whole list to fill the buffer.
 *
 * So enumeration is a VISITOR. Peak extra RAM is one entry, the backend streams
 * from state it already holds, and a caller with a bound stops early by
 * returning `false` — a normal outcome, not a truncation error. Same shape as
 * `process_raw_in_place` and `publish_streamed` already use here.
 *
 * Every string handed to a visitor is BORROWED for that call only.
 */

/** Visit one node. `enclave` is NULL where the backend does not track one —
 *  which is what lets a single slot answer both `rmw_get_node_names` and
 *  `rmw_get_node_names_with_enclaves`. Return `false` to stop. */
typedef bool (*rmw_node_visit_fn)(void *ctx, const char *node_name,
    const char *node_namespace, const char *enclave);

/** Visit one name and the types on it. `types_count` may legitimately be 0 on a
 *  partially discovered graph — reporting the name without a type beats
 *  dropping it. Return `false` to stop. */
typedef bool (*rmw_names_and_types_visit_fn)(void *ctx, const char *name,
    const char *const *types, size_t types_count);

/** Visit one discovered endpoint. Return `false` to stop. */
typedef bool (*rmw_topic_endpoint_info_visit_fn)(void *ctx,
    const rmw_topic_endpoint_info_t *info);

typedef struct nros_rmw_vtable_t {
    /* ---- Session lifecycle ---- */
    /** Create a session (phase-301: renamed from `open` to the table's
     *  own `create_*` convention). The runtime supplies a
     *  zero-initialised `rmw_session_t` via @p out with
     *  `node_name` / `namespace_` already filled. The backend writes
     *  `out->backend_data`.
     *
     *  @param mode One of `nros_rmw_session_mode_t`. Passed as `uint8_t`
     *              rather than the enum to keep the slot's width fixed
     *              across compilers. A backend with no peer/client
     *              distinction must IGNORE it, not reject it. */
    rmw_ret_t (*create_session)(const char *locator, uint8_t mode,
                           uint32_t domain_id, const char *node_name,
                           rmw_session_t *out);
    rmw_ret_t (*destroy_session)(rmw_session_t *session);
    rmw_ret_t (*drive_io)(rmw_session_t *session, int32_t timeout_ms);

    /* ---- Publisher ---- */
    /** Create a publisher. The runtime fills `out->topic_name`,
     *  `out->type_name`, `out->qos` before this call; the backend
     *  writes `out->backend_data` and `out->can_loan_messages`.
     *  `options` carries transport hints (phase-301: moved out of the
     *  QoS struct); NULL = all defaults. */
    rmw_ret_t (*create_publisher)(rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const rmw_qos_profile_t *qos,
        const rmw_publisher_options_t *options,
        rmw_publisher_t *out);
    void (*destroy_publisher)(rmw_publisher_t *publisher);
    rmw_ret_t (*publish)(rmw_publisher_t *publisher,
        const uint8_t *data, size_t len);

    /* ---- Subscription (phase-301: rmw's term; was `subscriber`) ---- */
    /** `options` carries transport hints (phase-301: moved out of the
     *  QoS struct); NULL = all defaults. */
    rmw_ret_t (*create_subscription)(rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const rmw_qos_profile_t *qos,
        const rmw_subscription_options_t *options,
        rmw_subscription_t *out);
    void (*destroy_subscription)(rmw_subscription_t *subscription);
    /** Upstream `rmw_take`. Phase 376 W3.b/W3.d step A.
     *
     *  `*taken` says whether a message was copied; `*out_len` is
     *  how many bytes, meaningful only when taken. Both are
     *  written only on `NROS_RMW_RET_OK`.
     *
     *  Second slot to retire `NROS_RMW_RET_NO_DATA`: an empty
     *  subscription is `taken = false` with OK, which is what
     *  upstream's `taken` out-parameter means.
     *
     *  Deviations from upstream, declared:
     *  - `buf` / `buf_len` / `*out_len` replace upstream's typed
     *    `void *ros_message`. There is no typesupport indirection
     *    on target — the payload is bytes and the caller owns the
     *    buffer, so it needs the length back.
     *  - no `rmw_subscription_allocation_t *`: pools are baked, so
     *    there is nothing to pre-size (the matching
     *    `rmw_init_subscription_allocation` is declined for the
     *    same reason). */
    rmw_ret_t (*take)(rmw_subscription_t *subscription,
        uint8_t *buf, size_t buf_len,
        size_t *out_len, bool *taken);
    /** Phase 376 W3.d step A — status in the return, answer in the
     *  out-parameter, so no slot multiplexes a flag with a status.
     *  `*out_has_data` is written only on `NROS_RMW_RET_OK`.
     *
     *  RTOS addition: upstream has no equivalent, because a hosted
     *  caller reaches for a wait-set. This is the poll a loop with
     *  no wait-set needs, and it allocates nothing.
     *
     *  A backend must not mutate subscription state here — the
     *  probe is logically read-only. */
    rmw_ret_t (*has_data)(rmw_subscription_t *subscription,
        bool *out_has_data);

    /* ---- Service (phase-301: rmw's term; was `service_server`) ---- */
    /* Phase 193.1b — `qos` applies to both the request + reply endpoints
       (one profile per service, mirrors create_publisher/subscription). */
    rmw_ret_t (*create_service)(rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const rmw_qos_profile_t *qos,
        rmw_service_t *out);
    void (*destroy_service)(rmw_service_t *server);
    /** Upstream `rmw_take_request`. Phase 376 W3.b/W3.d step A.
     *
     *  `*taken` says whether a request was copied, `*out_len` how
     *  many bytes, `*seq_out` the sequence number to reply against.
     *  All three are written only on `NROS_RMW_RET_OK`.
     *
     *  Deviations from upstream, declared: the payload is bytes
     *  (`buf` / `buf_len` / `*out_len`) rather than a typed
     *  `void *ros_request`, and `*seq_out` stands in for
     *  `rmw_service_info_t *` — an RTOS reply needs the sequence
     *  and nothing else in that struct. */
    rmw_ret_t (*take_request)(rmw_service_t *server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out,
        size_t *out_len, bool *taken);
    /** Phase 376 W3.d step A — the service-side sibling of
     *  `has_data`; same contract, same reason. */
    rmw_ret_t (*has_request)(rmw_service_t *server,
        bool *out_has_request);
    rmw_ret_t (*send_response)(rmw_service_t *server,
        int64_t seq, const uint8_t *data, size_t len);

    /* ---- Client (phase-301: rmw's term; was `service_client`) ---- */
    rmw_ret_t (*create_client)(rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const rmw_qos_profile_t *qos,
        rmw_client_t *out);
    void (*destroy_client)(rmw_client_t *client);

    /** Phase 130.4 — non-blocking send_request_raw. Phase-301: the
     *  deprecated blocking `call_raw` slot is DELETED (rmw has no
     *  blocking call); this + `try_recv_reply_raw` is the ONE
     *  request/reply path and both slots are now REQUIRED for a backend
     *  that supports services.
     *
     *  Sends the request to the backend without blocking for a
     *  reply. Returns immediately. */
    rmw_ret_t (*send_request)(rmw_client_t *client,
        const uint8_t *request, size_t req_len);

    /** Phase 130.4 — non-blocking try_recv_reply_raw.
     *
     *  Polls the backend for a reply. `>= 0` = reply bytes copied
     *  into `reply_buf`. `NROS_RMW_RET_NO_DATA` = no reply yet.
     *  Other negative = backend error. Paired with
     *  `send_request_raw` — backends implement both or neither. */
    /** Upstream `rmw_take_response`. Same shape and the same
     *  declared deviations as `take_request`. */
    rmw_ret_t (*take_response)(rmw_client_t *client,
        uint8_t *reply_buf, size_t reply_buf_len,
        size_t *out_len, bool *taken);

    /* ---- Phase 108 — status events (optional) ---- */
    /** Register a callback for a subscription-side event. NULL function
     *  pointer = backend doesn't generate any subscription events.
     *  Specific kind unsupported on a backend that supports some
     *  events = `NROS_RMW_RET_UNSUPPORTED` return.
     *  `deadline_ms` is consulted for `REQUESTED_DEADLINE_MISSED`
     *  only; ignored otherwise. */
    rmw_ret_t (*subscription_event_init)(
        rmw_subscription_t *subscription,
        rmw_event_type_t  kind,
        uint32_t               deadline_ms,
        rmw_status_event_callback_t cb,
        void                  *user_context);

    /** Register a callback for a publisher-side event. Same NULL /
     *  unsupported-kind conventions as `register_subscription_event`.
     *  `deadline_ms` is consulted for `OFFERED_DEADLINE_MISSED` only. */
    /* ---- Phase 376 W4 — why there is no `take_event` and no
     *      `event_set_callback` ----
     *
     * Upstream's status-event model is three-part: `*_event_init` fills an
     * `rmw_event_t` HANDLE, `rmw_event_set_callback` attaches a callback to that
     * handle, and `rmw_take_event` polls it for a status the wait set said was
     * ready. Ours fuses the first two and declines the third.
     *
     *  - `rmw_event_set_callback` is FUSED: our `*_event_init` takes the
     *    callback, so there is no handle to attach one to afterwards. What that
     *    costs is real and small — upstream can replace or clear a callback
     *    later, and we cannot.
     *
     *  - `rmw_take_event` is DECLINED, and the reason is not merely "no handle".
     *    Upstream polls an event because the WAIT SET told it one was ready, and
     *    the wait set is declined here (see the lifecycle block); without it a
     *    poll would be blind. More to the point, upstream's poll exists to move
     *    status handling OFF the notification context onto a safe one — and our
     *    callback already runs on the safe one: it is invoked from inside
     *    `drive_io`, on the executor thread, never from an ISR or a transport
     *    thread. The deferral upstream needs has already happened by the time
     *    the callback fires.
     */
    rmw_ret_t (*publisher_event_init)(
        rmw_publisher_t  *publisher,
        rmw_event_type_t  kind,
        uint32_t               deadline_ms,
        rmw_status_event_callback_t cb,
        void                  *user_context);

    /** Phase 108.B — manually assert this publisher's liveliness.
     *  Required for `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE` liveliness
     *  kinds; no-op (return `NROS_RMW_RET_OK`) for other kinds.
     *  NULL function pointer = backend doesn't support manual
     *  liveliness; runtime returns `NROS_RMW_RET_OK` for AUTOMATIC /
     *  NONE callers and `NROS_RMW_RET_UNSUPPORTED` for MANUAL_*. */
    rmw_ret_t (*publisher_assert_liveliness)(
        rmw_publisher_t *publisher);

    /** Phase 110.0 — backend's next internal-event deadline in
     *  milliseconds from now (lease keepalive, heartbeat, reader
     *  ACK-NACK timeout, etc.). The runtime caps its `drive_io`
     *  timeout against `min(user_timeout, timer_deadline, this)` so
     *  quiet links don't wake early, see no user-visible work, and
     *  round-trip back into `drive_io`.
     *
     *  Phase 376 W3.d step A — `*out_ms` carries the value and
     *  `*has_deadline` whether there is one; both are written only
     *  on `NROS_RMW_RET_OK`.
     *
     *  This slot was the ONE member of the eleven that step B's
     *  renumbering did not force: its old negative return was a
     *  "no deadline" SENTINEL, not an error code, so nothing would
     *  have collided. It is converted anyway because it had the
     *  shape every other conversion found a silent failure in — a
     *  backend that FAILED to compute its deadline returned `-1`
     *  and was read as "quiet link", which is exactly the reading
     *  that makes the executor sleep longer. It now has an error
     *  channel it never had.
     *
     *  NULL function pointer is permitted — the runtime treats it
     *  the same as `*has_deadline = false`. */
    rmw_ret_t (*next_deadline_ms)(const rmw_session_t *session,
        uint32_t *out_ms, bool *has_deadline);

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
    rmw_ret_t (*set_wake_callback)(rmw_session_t *session,
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
    rmw_ret_t (*borrow_loaned_message)(rmw_publisher_t *publisher,
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
    rmw_ret_t (*publish_loaned_message)(rmw_publisher_t *publisher,
                                  void                 *token,
                                  size_t                actual_len);

    /** Phase 124.A — abandon a previously loaned slot.
     *
     *  Releases the slot without sending. `token` MUST be a value
     *  returned from a prior `pub_loan` on the same publisher.
     *
     *  NULL = paired NULL with `pub_loan`. */
    void (*return_loaned_message_from_publisher)(rmw_publisher_t *publisher, void *token);

    /** Phase 124.A — zero-copy subscription borrow.
     *
     *  Borrow a read-only view of the next available message in
     *  place, without copying into a caller buffer. Returns:
     *    * `>= 0` — message length; writes `*out_buf` / `*out_token`.
     *    * `0` — no message ready (subscription empty).
     *    * `< 0` — error (see `rmw_ret_t` codes negated).
     *
     *  The view is valid until the matching `sub_release` runs.
     *  Only one borrow may be outstanding per subscription at a time —
     *  callers MUST release before requesting another borrow.
     *
     *  NULL function pointer = backend doesn't natively borrow; the
     *  runtime falls back to `try_recv_raw` into a staging buffer. */
    /** Upstream `rmw_take_loaned_message`. Phase 376 W3.b/W3.d step A.
     *
     *  `*taken` says whether a view was handed out; `*out_buf`,
     *  `*out_len` and `*out_token` describe it and are meaningful
     *  only when taken. All are written only on
     *  `NROS_RMW_RET_OK`.
     *
     *  Before this, the length was returned AND written to
     *  `*out_len`, and the runtime used the return — so a backend
     *  that disagreed with itself had one of its two answers
     *  silently ignored. There is now one length.
     *
     *  Deviation from upstream, declared: upstream loans a typed
     *  `void **loaned_message`; ours is a byte view plus an opaque
     *  token to release, because there is no typesupport on target
     *  and the backend owns the buffer until `sub_release`. */
    rmw_ret_t (*take_loaned_message)(rmw_subscription_t *subscription,
                           const uint8_t        **out_buf,
                           size_t                *out_len,
                           void                 **out_token,
                           bool                  *taken);

    /** Phase 124.A — release a previously borrowed view.
     *
     *  `token` MUST be a value returned from a prior `sub_borrow`
     *  on the same subscription. Lets the next message advance into
     *  the buffer.
     *
     *  NULL = paired NULL with `sub_borrow`. */
    void (*return_loaned_message_from_subscription)(rmw_subscription_t *subscription, void *token);

    /** Phase 124.C.1 — service-server availability probe.
     *
     *  Returns `1` if ≥ 1 matching server has been discovered on the
     *  RMW graph, `0` if none yet, or a negative `rmw_ret_t`
     *  constant on backend error. The runtime exposes this to user
     *  code as `nros_client_server_available()` /
     *  `Client<S>::server_available()` — clients use it to gate the
     *  first request so a startup-ordering race doesn't surface as
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
     *  surfaces `NROS_RMW_RET_UNSUPPORTED` to the caller.
     *
     *  Phase 376 W3.d step A — upstream's shape: the STATUS is the
     *  return value and the answer is an out-parameter. Previously
     *  this slot multiplexed both through one `int32_t` (1 = yes,
     *  0 = no, negative = error), which is what makes upstream's
     *  positive `RMW_RET_ERROR = 1` unadoptable — `1` would mean
     *  both "available" and "failed". Splitting them is what lets
     *  step B renumber at all.
     *
     *  A backend writes `*out_available` only on
     *  `NROS_RMW_RET_OK`; on any error the caller's value is
     *  untouched. The old contract's tolerance for "any positive
     *  value other than 1 means available" is gone with the int:
     *  a `bool` has no non-spec value to be lenient about.
     *
     *  Deviation from upstream, declared: no `node` parameter.
     *  `rmw_service_server_is_available` takes both a node and a
     *  client; an image has no node object to pass — the client
     *  reaches its session directly. */
    rmw_ret_t (*service_server_is_available)(
        rmw_client_t *client,
        bool *out_available);

    /** Phase 124.D.1 — burst-take.
     *
     *  Drains up to `max_msgs` queued messages into a contiguous
     *  caller buffer in a single backend call, avoiding N × vtable
     *  dispatch when a burst-sensor subscription catches up on a
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
     *    * `< 0` — `rmw_ret_t` error code; partial drains MUST
     *      use the count form, not error-out.
     *
     *  NULL function pointer = backend doesn't natively batch; the
     *  runtime emits a `try_recv_raw` loop fallback in
     *  `CffiSubscriber::try_recv_sequence`. The fallback gives
     *  identical observable behaviour (each call still costs N
     *  vtable hops) but lets user code commit to the batched API. */
    /** Upstream `rmw_take_sequence`. Phase 376 W3.b/W3.d step A —
     *  the COUNT moves to `*taken`, matching upstream's
     *  `size_t *taken`, and the return carries only a status.
     *  `*taken` is written only on `NROS_RMW_RET_OK`; a partial
     *  drain reports what it got rather than erroring. */
    rmw_ret_t (*take_sequence)(rmw_subscription_t *subscription,
                                  uint8_t              *buf,
                                  size_t                per_msg_cap,
                                  size_t                max_msgs,
                                  size_t               *out_lens,
                                  size_t               *taken);

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
    rmw_ret_t (*publish_streamed)(
        rmw_publisher_t *publisher,
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
    rmw_ret_t (*ping_session)(
        rmw_session_t *session,
        int32_t             timeout_ms);

    /* ---- Phase 231 (RFC-0038) — zero-copy in-place subscription take ---- */

    /** Capability query: does this subscription support process_raw_in_place()?
     *  Returns 1 if yes, 0 if no. The runtime consults this at subscription
     *  registration to choose in-place dispatch over the buffered (copying)
     *  path. NULL function pointer = treated as unsupported (buffered path). */
    /** Phase 376 W3.d step A — capability out, status returned.
     *  `*out_supports` is written only on `NROS_RMW_RET_OK`. */
    rmw_ret_t (*subscription_supports_in_place)(
        rmw_subscription_t *subscription,
        bool *out_supports);

    /** Borrow one ready message in place: hand its raw CDR bytes to `cb` (with
     *  the opaque `ctx`) for the duration of the call, then release the slot —
     *  no copy into a caller buffer. Returns 1 if a message was processed (`cb`
     *  invoked), NROS_RMW_RET_NO_DATA if none was ready, or a negative error.
     *  `cb` MUST NOT re-enter this subscription's receive. NULL function
     *  pointer = unsupported (the runtime uses the buffered path). */
    /** Phase 376 W3.d step A — "did it process one" moves to an
     *  out-parameter and the return is a plain status.
     *
     *  This retires `NROS_RMW_RET_NO_DATA` from this slot: an empty
     *  subscription is `*out_processed = false` with
     *  `NROS_RMW_RET_OK`, which is upstream's `taken = false`
     *  semantics. A sentinel that means "fine, but nothing" is
     *  exactly the shape that makes a status enum ambiguous.
     *
     *  `*out_processed` is written only on OK. */
    rmw_ret_t (*process_raw_in_place)(
        rmw_subscription_t *subscription,
        void                  *ctx,
        void                 (*cb)(void *ctx, const uint8_t *ptr, size_t len),
        bool                  *out_processed);
    /* ---- Phase 376 W4 — identity + matched counts (all optional) ---- */

    /** Upstream `rmw_get_implementation_identifier`.
     *
     *  The backend's name, static for the life of the image. A gid is only
     *  comparable with another carrying the same identifier, which matters here
     *  because `nros_rmw_cffi_register_named` admits several backends at once.
     *
     *  NULL slot: the runtime answers with the name the backend registered
     *  under, so this is a slot a backend only needs when it wants to report
     *  something other than its registry name. */
    const char *(*get_implementation_identifier)(void);

    /** Upstream `rmw_get_serialization_format`.
     *
     *  NULL slot: the runtime answers `"cdr"`. A backend overrides only if it
     *  speaks something else — and a bridge image linking two backends is
     *  exactly why this is per-BACKEND rather than a build-time constant. */
    const char *(*get_serialization_format)(void);

    /** Upstream `rmw_feature_supported`.
     *
     *  Whether the backend populates an optional piece of CONTENT — upstream's
     *  two values both concern message-info sequence numbers. Deliberately not
     *  expressed as slot nullity: a NULL pointer says the backend cannot
     *  perform an OPERATION, which is a different question from whether the
     *  data an implemented operation returns is populated.
     *
     *  NULL slot: the runtime answers `false` for every feature. */
    bool (*feature_supported)(rmw_feature_t feature);

    /** Upstream `rmw_get_gid_for_publisher`. Exact parity.
     *
     *  The backend zero-pads to the full width; see `rmw_gid_t`. */
    rmw_ret_t (*get_gid_for_publisher)(const rmw_publisher_t *publisher,
        rmw_gid_t *gid);

    /** Upstream `rmw_publisher_count_matched_subscriptions`. Exact parity.
     *
     *  Every backend already tracks this to implement liveliness events — see
     *  `service_server_is_available`, which is the same question one entity
     *  over. NULL where a backend has no discovery at all (XRCE). */
    rmw_ret_t (*publisher_count_matched_subscriptions)(
        const rmw_publisher_t *publisher, size_t *subscription_count);

    /** Upstream `rmw_subscription_count_matched_publishers`. Exact parity. */
    rmw_ret_t (*subscription_count_matched_publishers)(
        const rmw_subscription_t *subscription, size_t *publisher_count);

    /* ---- Phase 376 W4 — QoS read-back + clean shutdown (all optional) ---- */

    /** Upstream `rmw_publisher_get_actual_qos`. Exact parity.
     *
     *  We bake the REQUESTED profile and, until now, never read back the
     *  GRANTED one. On DDS the two differ whenever a writer and reader
     *  negotiate, and the difference is exactly what answers "why is nothing
     *  arriving" — so a consumer that cannot ask has to guess.
     *
     *  ALL-OR-NOTHING on purpose: a backend that can determine four policies
     *  and not the fifth returns `NROS_RMW_RET_UNSUPPORTED` and writes
     *  NOTHING, rather than filling what it knows. Our `rmw_qos_profile_t` has
     *  no `UNKNOWN`/`SYSTEM_DEFAULT` encoding, so a partial answer would be
     *  indistinguishable from a confident one. Adding those sentinels is an
     *  `rmw_entity.h` change and is booked for W5, not done here.
     *
     *  Six upstream entry points, six slots, deliberately: the name rule is
     *  mechanical so that no alias table has to be authored and kept true.
     *  Backends share ONE helper and write six one-line thunks — sharing an
     *  implementation is free, sharing an ABI slot is not. */
    rmw_ret_t (*publisher_get_actual_qos)(const rmw_publisher_t *publisher,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_subscription_get_actual_qos`. Exact parity. */
    rmw_ret_t (*subscription_get_actual_qos)(const rmw_subscription_t *subscription,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_client_request_publisher_get_actual_qos`. Exact parity.
     *
     *  The four service/client read-backs carry information available NOWHERE
     *  else: `rmw_client_t` and `rmw_service_t` have no `qos` field, and
     *  `create_client` / `create_service` take ONE profile for both
     *  directions, so the granted per-direction profile is otherwise
     *  unobservable. */
    rmw_ret_t (*client_request_publisher_get_actual_qos)(const rmw_client_t *client,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_client_response_subscription_get_actual_qos`. */
    rmw_ret_t (*client_response_subscription_get_actual_qos)(const rmw_client_t *client,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_service_request_subscription_get_actual_qos`. */
    rmw_ret_t (*service_request_subscription_get_actual_qos)(const rmw_service_t *service,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_service_response_publisher_get_actual_qos`. */
    rmw_ret_t (*service_response_publisher_get_actual_qos)(const rmw_service_t *service,
        rmw_qos_profile_t *qos);

    /** Upstream `rmw_publisher_wait_for_all_acked`.
     *
     *  Blocks until every sample this publisher sent has been acknowledged, or
     *  the timeout elapses. Without it an image that publishes and then halts
     *  cannot know whether anything left the box.
     *
     *  Deviation from upstream, declared: `uint32_t timeout_ms` for upstream's
     *  by-value `rmw_time_t`. Every duration in this ABI is u32 milliseconds
     *  (issue 0241) — one width, one unit, no per-call struct.
     *
     *  Best-effort backends (zenoh best-effort, XRCE) leave this NULL. */
    rmw_ret_t (*publisher_wait_for_all_acked)(const rmw_publisher_t *publisher,
        uint32_t timeout_ms);

    /* ---- Phase 376 W4 — with-info takes + entity callbacks (optional) ---- */

    /** Upstream `rmw_take_with_info`.
     *
     *  `take` plus the sample's metadata, written to caller-owned storage. See
     *  `rmw_message_info_t` for why this is a pointer parameter rather than the
     *  side table the runtime uses today.
     *
     *  Deviations from upstream, declared: the same two `take` declares —
     *  bytes (`buf`/`buf_len`/`*out_len`) instead of a typed `void *`, because
     *  there is no typesupport on target; and no allocation argument, because
     *  pools are baked.
     *
     *  NULL slot: the runtime falls back to `take`, and the caller gets no
     *  metadata — which is exactly today's behaviour for every C backend. */
    rmw_ret_t (*take_with_info)(rmw_subscription_t *subscription,
        uint8_t *buf, size_t buf_len,
        size_t *out_len, bool *taken, rmw_message_info_t *message_info);

    /** Upstream `rmw_take_loaned_message_with_info`.
     *
     *  `take_loaned_message` plus metadata; same deviations as that slot (a
     *  byte view and an opaque release token rather than a typed loan). */
    rmw_ret_t (*take_loaned_message_with_info)(rmw_subscription_t *subscription,
        const uint8_t **out_buf, size_t *out_len, void **out_token,
        bool *taken, rmw_message_info_t *message_info);

    /** Upstream `rmw_service_set_on_new_request_callback`. Exact parity.
     *
     *  Wake the executor when a request arrives, from the transport's own
     *  thread or an ISR. `set_wake_callback` is the SESSION-level sibling and
     *  does not cover this: it is installed once at open and says nothing about
     *  which entity woke. Without a per-entity callback a service-heavy image
     *  polls where a subscription-heavy one sleeps.
     *
     *  The callback is upstream's `rmw_event_callback_t` shape — which is why
     *  our DDS status-event callback had to give the name back
     *  (`rmw_status_event_callback_t`). */
    rmw_ret_t (*service_set_on_new_request_callback)(rmw_service_t *service,
        rmw_event_callback_t callback, const void *user_data);

    /** Upstream `rmw_client_set_on_new_response_callback`. Exact parity. */
    rmw_ret_t (*client_set_on_new_response_callback)(rmw_client_t *client,
        rmw_event_callback_t callback, const void *user_data);

    /** Upstream `rmw_subscription_set_on_new_message_callback`. Exact parity.
     *
     *  The third of the family, added with the other two rather than left
     *  recorded as "covered by `set_wake_callback`" — it never was: that slot
     *  is session-scoped and serves subscriptions, services and clients
     *  identically. */
    rmw_ret_t (*subscription_set_on_new_message_callback)(
        rmw_subscription_t *subscription,
        rmw_event_callback_t callback, const void *user_data);

    /* ---- Phase 376 W4 — graph introspection (all optional) ----
     *
     * NONE of these may block on the wire, and none takes a timeout: this
     * vtable's premise is that no background transport thread is assumed, so a
     * query-based backend keeps a `drive_io`-fed cache or leaves the slot NULL
     * rather than stalling the executor's only thread inside an introspection
     * call.
     *
     * A 128 KiB target cannot hold a graph cache, and NULL is the expected
     * answer there — the runtime surfaces UNSUPPORTED. XRCE cannot enumerate
     * participants at all.
     */

    /** Upstream `rmw_get_node_names` AND `rmw_get_node_names_with_enclaves`.
     *
     *  One slot, two upstream names: upstream split them only because appending
     *  to a fixed out-parameter list would have broken its ABI. A visitor has
     *  no such list, so the enclave is simply a fourth argument, NULL where
     *  untracked. Recorded in the checker's grouping table. */
    rmw_ret_t (*get_node_names)(const rmw_session_t *session,
        rmw_node_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_topic_names_and_types`. */
    rmw_ret_t (*get_topic_names_and_types)(const rmw_session_t *session,
        bool no_demangle, rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_service_names_and_types`. */
    rmw_ret_t (*get_service_names_and_types)(const rmw_session_t *session,
        rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_publisher_names_and_types_by_node`. */
    rmw_ret_t (*get_publisher_names_and_types_by_node)(const rmw_session_t *session,
        const char *node_name, const char *node_namespace, bool no_demangle,
        rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_subscriber_names_and_types_by_node`. */
    rmw_ret_t (*get_subscriber_names_and_types_by_node)(const rmw_session_t *session,
        const char *node_name, const char *node_namespace, bool no_demangle,
        rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_service_names_and_types_by_node`. */
    rmw_ret_t (*get_service_names_and_types_by_node)(const rmw_session_t *session,
        const char *node_name, const char *node_namespace,
        rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_client_names_and_types_by_node`. */
    rmw_ret_t (*get_client_names_and_types_by_node)(const rmw_session_t *session,
        const char *node_name, const char *node_namespace,
        rmw_names_and_types_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_publishers_info_by_topic`. */
    rmw_ret_t (*get_publishers_info_by_topic)(const rmw_session_t *session,
        const char *topic_name, bool no_mangle,
        rmw_topic_endpoint_info_visit_fn visit, void *ctx);

    /** Upstream `rmw_get_subscriptions_info_by_topic`. */
    rmw_ret_t (*get_subscriptions_info_by_topic)(const rmw_session_t *session,
        const char *topic_name, bool no_mangle,
        rmw_topic_endpoint_info_visit_fn visit, void *ctx);

    /** Upstream `rmw_count_publishers`. */
    rmw_ret_t (*count_publishers)(const rmw_session_t *session,
        const char *topic_name, size_t *count);

    /** Upstream `rmw_count_subscribers`. */
    rmw_ret_t (*count_subscribers)(const rmw_session_t *session,
        const char *topic_name, size_t *count);

    /** Upstream `rmw_node_get_graph_guard_condition`.
     *
     *  Registers a callback fired when the graph CHANGES. Upstream returns a
     *  guard condition the caller adds to a wait set; we have no wait set to
     *  add it to, and guard conditions are an executor concept here, so this is
     *  the `set_wake_callback` shape instead — the one guard condition whose
     *  trigger is genuinely backend knowledge.
     *
     *  The callback is an EDGE, carrying no payload: delivering WHAT changed
     *  would mean buffering it, which is the graph cache a small target cannot
     *  afford.
     *
     *  Named after upstream mechanically, per the campaign's rule, but the
     *  honest name for this shape is `set_on_graph_change_callback` — flagged
     *  for W5 rather than decided quietly here. */
    rmw_ret_t (*node_get_graph_guard_condition)(rmw_session_t *session,
        rmw_event_callback_t callback, const void *user_data);

    /* ---- Phase 376 W4 — graph node lifecycle (optional) ----
     *
     * The rest of upstream's lifecycle group is deliberately absent, and the
     * absences are decisions rather than omissions:
     *
     *  - `rmw_init` / `rmw_shutdown` / `rmw_context_fini` are
     *    `create_session` / `destroy_session`. There is no second teardown
     *    phase: the session shell is caller-owned, so there is nothing left to
     *    free after the backend releases `backend_data`.
     *
     *  - `rmw_init_options_{init,copy,fini}` have nothing to do here. Upstream
     *    needs the trio because its options OWN heap and carry an
     *    `rcutils_allocator_t`, which cannot cross this seam; ours is a
     *    build-time POD, so "copy" is `=` and "fini" is nothing. (This does NOT
     *    decide `security_options` / `discovery_options`, which we answer
     *    nowhere — see the phase doc.)
     *
     *  - `rmw_wait`, the wait set, and the guard conditions are declined
     *    together, because `has_data` / `has_request` + `drive_io` +
     *    `set_wake_callback` + `next_deadline_ms` ARE upstream's `rmw_wait`,
     *    decomposed. What a vtable `wait` would add is only the BLOCK, moved
     *    from the platform into the backend — and the block cannot live there:
     *    one executor drives sessions from several backends at once
     *    (`create_node_on(name, rmw)`), timers fire off the platform clock, and
     *    guard conditions fire from another thread or an ISR. A backend can
     *    only block on its own handles. `next_deadline_ms` is exactly the piece
     *    of `rmw_wait` that CANNOT be answered above the seam, and it is
     *    already a slot; that is the decomposition working, not a gap.
     */

    /** Upstream `rmw_create_node`.
     *
     *  Declares a node on the graph. NULL slot is the expected implementation
     *  in a static image: the runtime still tracks the node, the backend simply
     *  has nothing to declare.
     *
     *  Deviations from upstream, declared: no `rmw_context_t *` (an image has
     *  one session and reaches it directly), and the node is an OUT parameter
     *  rather than a returned pointer — no runtime allocation, the caller owns
     *  the storage, exactly as `create_publisher` does.
     *
     *  The runtime calls this once per distinct `(name, namespace_)`. */
    rmw_ret_t (*create_node)(rmw_session_t *session,
        const char *name, const char *namespace_,
        rmw_node_t *out);

    /** Upstream `rmw_destroy_node`.
     *
     *  Releases the backend's `backend_data`; the shell stays valid until its
     *  owner drops it. */
    rmw_ret_t (*destroy_node)(rmw_node_t *node);

} nros_rmw_vtable_t;

/**
 * Session mode for `create_session`'s @p mode parameter (issue 0331).
 *
 * These values were previously an undocumented bare `uint8_t` with no legal-
 * value list — the only slot in the vtable without one — encoded inline as
 * `0u8` / `1u8` at the Rust boundary.
 *
 * Divergence from `rmw.h`, recorded deliberately: `rmw_init_options_t` carries
 * domain_id, enclave, security_options and discovery_options, and has NO
 * session-mode concept. This parameter is closest to zenoh's `whatami`, and a
 * backend that has no such notion (cyclonedds, XRCE) is expected to IGNORE it
 * rather than fail. Folding it into backend-private config behind the locator
 * — so the agnostic vtable stops carrying a backend-shaped field — is the
 * structural fix, and is not done here; see issue 0331.
 */
typedef enum nros_rmw_session_mode_t {
    /** Connect to a router/agent as a client. The default. */
    NROS_RMW_SESSION_MODE_CLIENT = 0,
    /** Peer-to-peer, no router. Backends without a peer mode ignore this. */
    NROS_RMW_SESSION_MODE_PEER = 1,
} nros_rmw_session_mode_t;

/** Register a custom RMW backend under the implicit name "default".
 *  Legacy single-arg form retained for source compatibility with
 *  backend ctors authored before the named registry (Phase 104.B.2).
 *
 *  Deprecated (Phase 128.B.5): every in-tree backend now calls
 *  `nros_rmw_cffi_register_named` with its canonical name. The
 *  unnamed shim will be removed in a follow-up phase.
 *  Returns NROS_RMW_RET_OK. */
rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

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
rmw_ret_t nros_rmw_cffi_register_named(const char *name,
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

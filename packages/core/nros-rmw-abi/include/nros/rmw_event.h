#ifndef NROS_RMW_EVENT_H
#define NROS_RMW_EVENT_H

#include <stdint.h>
#include <stddef.h>

#include "nros/rmw_ret.h"

/**
 * @file rmw_event.h
 * @brief Tier-1 status events: liveliness changes, deadline misses,
 *        message loss.
 *
 * The status-event surface for the nros RMW C vtable. Backends
 * advertise per-event support; applications register a callback per
 * (entity, event kind) pair.
 *
 * **Dispatch model.** Callback-on-entity. Backends fire registered
 * callbacks from inside `drive_io` when the event is detected — same
 * thread, same priority, same constraints as message callbacks. No
 * waitset, no take-event polling. See
 * `book/src/concepts/status-events.md` and
 * `book/src/design/rmw-vs-upstream.md` Section 8 for the design.
 *
 * **Tier-2 / Tier-3 events skipped.** `MATCHED` (Tier-2) is deferred
 * until dynamic-discovery use cases appear — additive without ABI
 * break (the enum is integer-valued; unknown values pass through).
 * `QOS_INCOMPATIBLE` and `INCOMPATIBLE_TYPE` (Tier-3) are surfaced
 * synchronously at create-time as `rmw_ret_t` codes
 * (`NROS_RMW_RET_INCOMPATIBLE_QOS`, `NROS_RMW_RET_TOPIC_NAME_INVALID`)
 * rather than as runtime events.
 */

/** Tier-1 event kinds. Stable integer values; future kinds (Tier-2)
 *  extend the enum at end. */
typedef enum rmw_event_type_t {
    /** Subscriber: a tracked publisher's liveliness state changed. */
    NROS_RMW_EVENT_LIVELINESS_CHANGED         = 0,
    /** Subscriber: an expected sample didn't arrive within the
     *  configured deadline. */
    NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED  = 1,
    /** Subscriber: backend dropped a sample (overflow / etc.). */
    NROS_RMW_EVENT_MESSAGE_LOST               = 2,
    /** Publisher: this publisher missed its own liveliness assertion. */
    NROS_RMW_EVENT_LIVELINESS_LOST            = 3,
    /** Publisher: this publisher promised X Hz, fell behind. */
    NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED    = 4,
} rmw_event_type_t;

/** Liveliness payload. Mirrors the DDS
 *  `rmw_liveliness_changed_status_t` shape. */
typedef struct rmw_liveliness_changed_status_t {
    uint16_t alive_count;
    uint16_t not_alive_count;
    int16_t  alive_count_change;
    int16_t  not_alive_count_change;
} rmw_liveliness_changed_status_t;

/** Count payload. Used for `MESSAGE_LOST`,
 *  `REQUESTED_DEADLINE_MISSED`, `LIVELINESS_LOST`,
 *  `OFFERED_DEADLINE_MISSED` — all share the same shape. */
typedef struct rmw_count_status_t {
    uint32_t total_count;
    uint32_t total_count_change;
} rmw_count_status_t;

/** Borrow-shaped union the backend supplies to the registered
 *  callback. The `kind` argument selects which member is valid. */
typedef union rmw_event_payload_t {
    rmw_liveliness_changed_status_t liveliness_changed;
    rmw_count_status_t              count;
} rmw_event_payload_t;

/**
 * User callback invoked when an event fires.
 *
 * @param kind          Identifies which member of @p payload is valid.
 * @param payload       Pointer is valid for the duration of this call
 *                      only — copy fields if needed beyond return.
 * @param user_context  Opaque pointer registered with the callback.
 *
 * **Threading.** Invoked from inside `drive_io` on the executor
 * thread. Must not block; long work should defer via a guard
 * condition or queue.
 */
/* Phase 376 W5 — `rmw_status_event_callback_t`, NOT `rmw_event_callback_t`.
 *
 * Upstream binds `rmw_event_callback_t` to a DIFFERENT signature —
 * `void(const void *user_data, size_t number_of_events)`, the callback
 * `rmw_{service,client,subscription}_set_on_new_*_callback` install. Ours is
 * the DDS STATUS-event callback: a kind, a payload union, and a context.
 *
 * Two types of one name whose shapes disagree is exactly the hazard
 * `rmw_vtable.h`'s `#error` guard exists for — except this one would have been
 * INSIDE our own header, where the guard cannot see it, and
 * `scripts/rmw-abi-shape.py` compares parameter type NAMES, so it would have
 * reported the `set_on_new_*` slots as matching upstream exactly while they
 * took a callback of the wrong shape. Introduced by W3.a's rename; caught by
 * the W5 audit before any slot depended on it. */
typedef void (*rmw_status_event_callback_t)(
    rmw_event_type_t            kind,
    const rmw_event_payload_t  *payload,
    void                            *user_context);


/** Upstream `rmw_event_callback_t` — the callback the `set_on_new_*` slots
 *  install. Distinct from `rmw_status_event_callback_t` above, which is the DDS
 *  STATUS-event callback; upstream binds this name to this shape and we now
 *  match it. */
typedef void (*rmw_event_callback_t)(const void *user_data, size_t number_of_events);

#endif /* NROS_RMW_EVENT_H */

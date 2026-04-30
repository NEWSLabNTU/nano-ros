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
 * synchronously at create-time as `nros_rmw_ret_t` codes
 * (`NROS_RMW_RET_INCOMPATIBLE_QOS`, `NROS_RMW_RET_TOPIC_NAME_INVALID`)
 * rather than as runtime events.
 */

/** Tier-1 event kinds. Stable integer values; future kinds (Tier-2)
 *  extend the enum at end. */
typedef enum nros_rmw_event_kind_t {
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
} nros_rmw_event_kind_t;

/** Liveliness payload. Mirrors the DDS
 *  `rmw_liveliness_changed_status_t` shape. */
typedef struct nros_rmw_liveliness_changed_status_t {
    uint16_t alive_count;
    uint16_t not_alive_count;
    int16_t  alive_count_change;
    int16_t  not_alive_count_change;
} nros_rmw_liveliness_changed_status_t;

/** Count payload. Used for `MESSAGE_LOST`,
 *  `REQUESTED_DEADLINE_MISSED`, `LIVELINESS_LOST`,
 *  `OFFERED_DEADLINE_MISSED` — all share the same shape. */
typedef struct nros_rmw_count_status_t {
    uint32_t total_count;
    uint32_t total_count_change;
} nros_rmw_count_status_t;

/** Borrow-shaped union the backend supplies to the registered
 *  callback. The `kind` argument selects which member is valid. */
typedef union nros_rmw_event_payload_t {
    nros_rmw_liveliness_changed_status_t liveliness_changed;
    nros_rmw_count_status_t              count;
} nros_rmw_event_payload_t;

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
typedef void (*nros_rmw_event_callback_t)(
    nros_rmw_event_kind_t            kind,
    const nros_rmw_event_payload_t  *payload,
    void                            *user_context);

#endif /* NROS_RMW_EVENT_H */

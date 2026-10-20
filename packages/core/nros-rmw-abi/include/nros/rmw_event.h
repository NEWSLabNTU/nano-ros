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
 * **Matched and incompatible-type events (issue 1972).** Iron/Jazzy's
 * RMW_EVENT_{PUBLICATION,SUBSCRIPTION}_MATCHED and
 * RMW_EVENT_{PUBLISHER,SUBSCRIPTION}_INCOMPATIBLE_TYPE are values 5..8,
 * APPENDED. Upstream inserted them in the middle of its enum; ours was never
 * value-compatible with upstream's (ours numbers LIVELINESS_LOST 3, upstream
 * numbers it 6), so appending keeps every existing value and every compiled
 * backend's switch arms. They travel through the same `*_event_init` /
 * `*_take_event` slots as the Tier-1 kinds — no new slot.
 *
 * Upstream's QOS_INCOMPATIBLE is still surfaced synchronously at
 * create-time as `NROS_RMW_RET_INCOMPATIBLE_QOS` rather than as a runtime
 * event.
 */

/** Status event kinds. Stable integer values; new kinds extend the enum at
 *  the END, never in the middle. */
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
    /** Publisher: the set of subscriptions matched to it changed (one
     *  appeared or left). Payload: `matched`. Upstream
     *  RMW_EVENT_PUBLICATION_MATCHED (Iron+), issue 1972. */
    NROS_RMW_EVENT_PUBLICATION_MATCHED        = 5,
    /** Subscription: the set of publishers matched to it changed. Payload:
     *  `matched`. Upstream RMW_EVENT_SUBSCRIPTION_MATCHED. */
    NROS_RMW_EVENT_SUBSCRIPTION_MATCHED       = 6,
    /** Publisher: an endpoint on the same topic was seen with a DIFFERENT
     *  type. Payload: `incompatible_type`. Upstream
     *  RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE. */
    NROS_RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE    = 7,
    /** Subscription: an endpoint on the same topic was seen with a
     *  DIFFERENT type. Payload: `incompatible_type`. Upstream
     *  RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE. */
    NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE = 8,
} rmw_event_type_t;

/** Liveliness payload. Mirrors the DDS
 *  `rmw_liveliness_changed_status_t` shape. */
typedef struct rmw_liveliness_changed_status_t {
    uint16_t alive_count;
    uint16_t not_alive_count;
    int16_t  alive_count_change;
    int16_t  not_alive_count_change;
} rmw_liveliness_changed_status_t;

/** Count payload. Used for `NROS_RMW_EVENT_MESSAGE_LOST`,
 *  `NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED`,
 *  `NROS_RMW_EVENT_LIVELINESS_LOST`,
 *  `NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED` — all share the same shape. */
typedef struct rmw_count_status_t {
    uint32_t total_count;
    uint32_t total_count_change;
} rmw_count_status_t;

/** Matched payload — `NROS_RMW_EVENT_PUBLICATION_MATCHED` /
 *  `NROS_RMW_EVENT_SUBSCRIPTION_MATCHED`. Upstream `rmw_matched_status_t`
 *  (Iron+), same four fields in the same order.
 *
 *  Width is the one deviation: upstream's three counts are `size_t`; ours
 *  are `uint32_t`, like every other count in this header, so the union keeps
 *  one layout on 32- and 64-bit targets. `current_count_change` is `int32_t`
 *  on both sides. */
typedef struct rmw_matched_status_t {
    /** Cumulative matches over the entity's lifetime. */
    uint32_t total_count;
    /** Change in `total_count` since the last report. */
    uint32_t total_count_change;
    /** Endpoints matched RIGHT NOW. */
    uint32_t current_count;
    /** Change in `current_count` since the last report; negative when an
     *  endpoint left. */
    int32_t  current_count_change;
} rmw_matched_status_t;

/** Incompatible-type payload — `NROS_RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE`
 *  / `NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE`. Upstream
 *  `rmw_incompatible_type_status_t` (Iron+); upstream's fields are `int32_t`,
 *  ours `uint32_t` like `rmw_count_status_t`, since both only grow. */
typedef struct rmw_incompatible_type_status_t {
    uint32_t total_count;
    uint32_t total_count_change;
} rmw_incompatible_type_status_t;

/** Borrow-shaped union the backend supplies to the registered
 *  callback. The `kind` argument selects which member is valid.
 *
 *  Members are APPENDED (issue 1972 grew it from 8 to 16 bytes). A caller
 *  owns the union it hands `*_take_event`, so a backend built against the
 *  older, smaller header writes within the larger one. */
typedef union rmw_event_payload_t {
    rmw_liveliness_changed_status_t liveliness_changed;
    rmw_count_status_t              count;
    rmw_matched_status_t            matched;
    rmw_incompatible_type_status_t  incompatible_type;
} rmw_event_payload_t;

/**
 * User callback invoked when an event fires.
 *
 * @param kind          Identifies which member of @p payload is valid.
 * @param payload       Pointer is valid for the duration of this call
 *                      only — copy fields if needed beyond return.
 * @param user_context  Opaque pointer registered with the callback.
 *
 * **Threading.** Invoked on whatever context the BACKEND delivers on, which
 * is not uniform and is not `drive_io`. This paragraph claimed "invoked from
 * inside `drive_io` on the executor thread" until 2026-08-25; no backend did
 * that, and the claim was load-bearing — it was the stated reason
 * `rmw_take_event` could be declined (issue 0780). zenoh fires from
 * `take` and `has_data`; a DDS backend's listeners fire on its own
 * worker thread.
 *
 * So: must not block, and must assume it may be running concurrently with the
 * executor. A backend with no safe context to deliver from should leave
 * `*_event_init` NULL and implement `*_take_event` instead, which hands the
 * caller the deferral upstream gets from its wait set.
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

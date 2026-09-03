// QoS mapping: rmw_qos_profile_t → dds_qos_t.
//
// Phase 117.6 — applies the full DDS-shaped subset (reliability,
// durability, history+depth, deadline, lifespan, liveliness +
// lease). Cyclone honours every policy in `rmw_qos_profile_t`, so no
// per-policy support mask is exposed yet.

#include "qos.hpp"

#include <dds/dds.h>

#include "nros/rmw_entity.h"

namespace nros_rmw_cyclonedds {

dds_qos_t *make_dds_qos(const rmw_qos_profile_t *src) {
    if (src == nullptr) {
        return nullptr;
    }
    dds_qos_t *q = dds_create_qos();
    if (q == nullptr) {
        return nullptr;
    }

    // issue 0829 — the test is on BEST_EFFORT, not on RELIABLE, because this
    // field has FOUR values and only one of them means best-effort.
    // `NROS_RMW_RELIABILITY_SYSTEM_DEFAULT` is 0 and `_UNKNOWN` is 3
    // (`nros/rmw_entity.h:70-76`); testing `== RELIABLE` sent both to the
    // else-branch, so a zero-filled or hand-rolled `rmw_qos_profile_t` — a
    // shape the header explicitly contemplates — got BEST_EFFORT here while
    // the tree's two other folds of the same value got RELIABLE
    // (`nros-rmw-xrce/src/session.c:211`,
    // `nros-rmw-cffi/src/rust_adapter.rs:258`). Upstream settles it the same
    // way: `rmw_cyclonedds_cpp`'s `create_readwrite_qos` puts
    // `RELIABILITY_SYSTEM_DEFAULT` on the `RELIABLE` case by fallthrough.
    // Cyclone is the backend that meets real ROS peers, and it was the one
    // picking the less safe side.
    dds_qset_reliability(
        q,
        src->reliability == NROS_RMW_RELIABILITY_BEST_EFFORT
            ? DDS_RELIABILITY_BEST_EFFORT
            : DDS_RELIABILITY_RELIABLE,
        // Default max blocking time on reliable: 1 s. 100 ms (the
        // pre-117.X.5 value) was too aggressive for the local
        // participant's reader-writer match handshake between
        // concurrent service clients — Cyclone's SEDP propagation
        // across SMP cores routinely takes 100–500 ms on POSIX.
        // 1 s matches typical `rmw_cyclonedds_cpp` deployments.
        // `rmw_qos_profile_t` doesn't expose this knob in v1 — surface
        // it through the reserved bytes if a tighter bound matters.
        DDS_SECS(1));

    dds_qset_durability(
        q,
        src->durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL
            ? DDS_DURABILITY_TRANSIENT_LOCAL
            : DDS_DURABILITY_VOLATILE);

    // issue 0829 — the history KIND needs no new arm: the else-branch already
    // sends `NROS_RMW_HISTORY_SYSTEM_DEFAULT` (0) to KEEP_LAST, which is where
    // `rmw_cyclonedds_cpp` puts it too (its `HISTORY_SYSTEM_DEFAULT` joins
    // `KEEP_LAST` by case fallthrough).
    //
    // The DEPTH does. `NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT` now carries depth
    // 0, and Cyclone does not clamp that — `validate_history_qospolicy`
    // REJECTS `KEEP_LAST` with `depth < 1` outright
    // (`ddsi_plist.c:2603-2604`, reached from every entity-create path via
    // `ddsi_xqos_valid`), so a sentinel profile would fail create with
    // `BAD_PARAMETER` rather than defer to anything. 1 is what
    // `create_readwrite_qos` resolves `RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT`
    // to — `dds_qset_history(qos, DDS_HISTORY_KEEP_LAST, 1)` — and it is also
    // Cyclone's own reader/writer table default (`ddsi_default_qos_reader`,
    // `ddsi_plist.c:3454-3455`), so upstream and the middleware agree here
    // even though they do not on reliability.
    //
    // KEEP_ALL keeps its 0: it carries one legitimately (upstream's
    // `rmw_qos_profile_parameter_events` does) and the validator constrains
    // only the KEEP_LAST case.
    const bool keep_all = (src->history == NROS_RMW_HISTORY_KEEP_ALL);
    uint32_t depth = src->depth;
    if (!keep_all && depth == 0) {
        depth = 1;
    }
    dds_qset_history(
        q,
        keep_all ? DDS_HISTORY_KEEP_ALL : DDS_HISTORY_KEEP_LAST,
        (int32_t)depth);

    // Phase-301 (issue 0241): `NROS_RMW_DURATION_INFINITE_MS` is the
    // explicit infinite spelling — semantically identical to 0 (no
    // check) at every duration comparison.
    if (src->deadline_ms != 0 && src->deadline_ms != NROS_RMW_DURATION_INFINITE_MS) {
        dds_qset_deadline(q, DDS_MSECS(src->deadline_ms));
    }
    if (src->lifespan_ms != 0 && src->lifespan_ms != NROS_RMW_DURATION_INFINITE_MS) {
        dds_qset_lifespan(q, DDS_MSECS(src->lifespan_ms));
    }

    if (src->liveliness_kind != NROS_RMW_LIVELINESS_SYSTEM_DEFAULT) {
        dds_liveliness_kind_t k = DDS_LIVELINESS_AUTOMATIC;
        switch (src->liveliness_kind) {
            case NROS_RMW_LIVELINESS_AUTOMATIC:
                k = DDS_LIVELINESS_AUTOMATIC;
                break;
            case NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC:
                k = DDS_LIVELINESS_MANUAL_BY_TOPIC;
                break;
            case NROS_RMW_LIVELINESS_MANUAL_BY_NODE:
                // Cyclone has no MANUAL_BY_NODE; fold to MANUAL_BY_TOPIC.
                k = DDS_LIVELINESS_MANUAL_BY_TOPIC;
                break;
            default:
                k = DDS_LIVELINESS_AUTOMATIC;
                break;
        }
        const dds_duration_t lease =
            (src->liveliness_lease_ms != 0 &&
             src->liveliness_lease_ms != NROS_RMW_DURATION_INFINITE_MS)
            ? DDS_MSECS(src->liveliness_lease_ms)
            : DDS_INFINITY;
        dds_qset_liveliness(q, k, lease);
    }

    return q;
}

/* issue 0823 — read back what the participant ACTUALLY holds.
 *
 * QoS is a negotiation and this runtime used to report the value it REQUESTED
 * as the value it got: all six `*_get_actual_qos` slots were inert, so a
 * downgrade (the RELIABLE reader that matched a BEST_EFFORT writer, the depth
 * the middleware clamped) was invisible at every layer. Silence from a QoS
 * mismatch is indistinguishable from a name typo, a domain split (issue 0801)
 * or a discovery failure (issue 0803) — all three cost hours this month.
 *
 * The inverse of `make_dds_qos`. Fields Cyclone does not report keep the value
 * the caller passed in, so an unreported field reads as "unchanged" rather than
 * as zero — a zeroed `depth` would look like a legitimate answer.
 */
rmw_ret_t read_entity_qos(dds_entity_t entity, rmw_qos_profile_t *out) {
    if (entity <= 0 || out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_qos_t *q = dds_create_qos();
    if (q == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    if (dds_get_qos(entity, q) != DDS_RETCODE_OK) {
        dds_delete_qos(q);
        return NROS_RMW_RET_ERROR;
    }

    dds_reliability_kind_t rel;
    dds_duration_t max_block;
    if (dds_qget_reliability(q, &rel, &max_block)) {
        out->reliability = (rel == DDS_RELIABILITY_RELIABLE)
                               ? NROS_RMW_RELIABILITY_RELIABLE
                               : NROS_RMW_RELIABILITY_BEST_EFFORT;
    }

    dds_durability_kind_t dur;
    if (dds_qget_durability(q, &dur)) {
        out->durability = (dur == DDS_DURABILITY_VOLATILE)
                              ? NROS_RMW_DURABILITY_VOLATILE
                              : NROS_RMW_DURABILITY_TRANSIENT_LOCAL;
    }

    dds_history_kind_t hist;
    int32_t depth = 0;
    if (dds_qget_history(q, &hist, &depth)) {
        out->history = (hist == DDS_HISTORY_KEEP_ALL) ? NROS_RMW_HISTORY_KEEP_ALL
                                                      : NROS_RMW_HISTORY_KEEP_LAST;
        /* KEEP_ALL reports no meaningful depth; leave the requested value
         * rather than writing a 0 that reads as an answer. */
        if (hist != DDS_HISTORY_KEEP_ALL && depth > 0) {
            out->depth = (depth > 0xFFFF) ? 0xFFFFu : (uint16_t)depth;
        }
    }

    dds_duration_t dl = 0;
    if (dds_qget_deadline(q, &dl)) {
        out->deadline_ms = (dl == DDS_INFINITY) ? NROS_RMW_DURATION_INFINITE_MS
                                                : (uint32_t)(dl / DDS_NSECS_IN_MSEC);
    }
    dds_duration_t ls = 0;
    if (dds_qget_lifespan(q, &ls)) {
        out->lifespan_ms = (ls == DDS_INFINITY) ? NROS_RMW_DURATION_INFINITE_MS
                                                : (uint32_t)(ls / DDS_NSECS_IN_MSEC);
    }

    dds_liveliness_kind_t lk;
    dds_duration_t lease = 0;
    if (dds_qget_liveliness(q, &lk, &lease)) {
        /* MANUAL_BY_NODE folds to MANUAL_BY_TOPIC on the way IN (Cyclone has no
         * MANUAL_BY_NODE), so it cannot come back out — reporting
         * MANUAL_BY_TOPIC is the truth about the entity, not a lossy round
         * trip. */
        out->liveliness_kind = (lk == DDS_LIVELINESS_AUTOMATIC)
                                   ? NROS_RMW_LIVELINESS_AUTOMATIC
                                   : NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC;
        out->liveliness_lease_ms = (lease == DDS_INFINITY)
                                       ? NROS_RMW_DURATION_INFINITE_MS
                                       : (uint32_t)(lease / DDS_NSECS_IN_MSEC);
    }

    dds_delete_qos(q);
    return NROS_RMW_RET_OK;
}

} // namespace nros_rmw_cyclonedds

// QoS mapping: nros_rmw_qos_t → dds_qos_t.
//
// Phase 117.6 — applies the full DDS-shaped subset (reliability,
// durability, history+depth, deadline, lifespan, liveliness +
// lease). Cyclone honours every policy in `nros_rmw_qos_t`, so no
// per-policy support mask is exposed yet.

#include "qos.hpp"

#include <dds/dds.h>

#include "nros/rmw_entity.h"

namespace nros_rmw_cyclonedds {

dds_qos_t *make_dds_qos(const nros_rmw_qos_t *src) {
    if (src == nullptr) {
        return nullptr;
    }
    dds_qos_t *q = dds_create_qos();
    if (q == nullptr) {
        return nullptr;
    }

    dds_qset_reliability(
        q,
        src->reliability == NROS_RMW_RELIABILITY_RELIABLE
            ? DDS_RELIABILITY_RELIABLE
            : DDS_RELIABILITY_BEST_EFFORT,
        // Default max blocking time on reliable: 100 ms. Matches what
        // upstream rmw_cyclonedds_cpp does. nros_rmw_qos_t doesn't
        // expose this knob in v1 — surface it through the reserved
        // bytes in a follow-up if anyone needs to tune it.
        DDS_MSECS(100));

    dds_qset_durability(
        q,
        src->durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL
            ? DDS_DURABILITY_TRANSIENT_LOCAL
            : DDS_DURABILITY_VOLATILE);

    dds_qset_history(
        q,
        src->history == NROS_RMW_HISTORY_KEEP_ALL ? DDS_HISTORY_KEEP_ALL
                                                  : DDS_HISTORY_KEEP_LAST,
        src->depth);

    if (src->deadline_ms != 0) {
        dds_qset_deadline(q, DDS_MSECS(src->deadline_ms));
    }
    if (src->lifespan_ms != 0) {
        dds_qset_lifespan(q, DDS_MSECS(src->lifespan_ms));
    }

    if (src->liveliness_kind != NROS_RMW_LIVELINESS_NONE) {
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
        const dds_duration_t lease = src->liveliness_lease_ms != 0
            ? DDS_MSECS(src->liveliness_lease_ms)
            : DDS_INFINITY;
        dds_qset_liveliness(q, k, lease);
    }

    return q;
}

} // namespace nros_rmw_cyclonedds

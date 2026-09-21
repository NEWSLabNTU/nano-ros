// issue 1329 — uORB's QoS answer, and what makes it an answer rather than a
// shrug.
//
// Until this file existed, every uORB create bound its profile as
// `const rmw_qos_profile_t * /*qos*/` and the cffi route advertised, on this
// backend's behalf, the UNION of what any nano-ros-supported RMW honours. So a
// PX4 module asking for TRANSIENT_LOCAL, a deadline or a 1000-deep history was
// admitted and then given a shared-memory ring with none of it — the
// no-silent-downgrade contract broken one layer below where it is enforced.
//
// The honest mask for this backend is not "nothing". uORB is a shared-memory
// broker with a per-topic ring, and three of the four CORE policies have a
// real answer here:
//
//   * RELIABILITY. Publisher and subscriber are the same process's memory.
//     There is no lossy channel between them and nothing to retransmit over: a
//     subscriber reading within the topic's queue depth sees every sample, in
//     order. That is RELIABLE delivery, bounded by history exactly as a DDS
//     RELIABLE + KEEP_LAST(N) writer is. BEST_EFFORT is over-delivered, which
//     is the safe direction and the same grant the zenoh shim makes.
//   * DURABILITY. No cache for late joiners beyond what the ring holds, so
//     VOLATILE is served and TRANSIENT_LOCAL is REFUSED.
//   * HISTORY. The ring is KEEP_LAST by construction, so KEEP_ALL is REFUSED.
//     A KEEP_ALL request on a bus that overwrites is the case where a silent
//     downgrade costs data.
//   * DEPTH is the interesting one. The queue length belongs to the TOPIC —
//     `orb_metadata::o_queue`, fixed at `ORB_DEFINE` time — so a deeper
//     request cannot be served and refusing it would refuse
//     `QOS_PROFILE_DEFAULT`, which asks for 10 against PX4 topics that
//     mostly declare 1. It is GRANTED DOWN, and the grant is reported through
//     `publisher_get_actual_qos` / `subscription_get_actual_qos`, which this
//     backend now fills and which the runtime reads at create (issues 0823 and
//     1327). Granted-and-reported is a different thing from clamped-and-silent;
//     this backend does the first.
//
// Deadline, lifespan, liveliness and `avoid_ros_namespace_conventions` have no
// answer here at all and are not claimed. They are refused at create now,
// naming the policy, instead of being accepted and ignored.

#include "internal.hpp"
#include "uorb_abi.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

rmw_ret_t qos_admit(const rmw_qos_profile_t* qos) {
    // A NULL profile is "the caller stated nothing", which every backend
    // serves. `create_*` already treats it that way for every other field.
    if (qos == nullptr) {
        return NROS_RMW_RET_OK;
    }

    // nros-qos-honours: RELIABILITY
    //
    // Read and decided, not waved through: the two real values are both
    // served (see the header block — shared memory loses nothing within the
    // ring), and `UNKNOWN` is a sentinel a *_get_actual_qos answer carries,
    // never a request this backend can serve.
    switch (qos->reliability) {
    case NROS_RMW_RELIABILITY_SYSTEM_DEFAULT:
    case NROS_RMW_RELIABILITY_RELIABLE:
    case NROS_RMW_RELIABILITY_BEST_EFFORT:
        break;
    default:
        return NROS_RMW_RET_INCOMPATIBLE_QOS;
    }

    // nros-qos-honours: DURABILITY_VOLATILE
    //
    // TRANSIENT_LOCAL asks the broker to hand a late joiner the samples it
    // missed; uORB keeps no such cache, so it is refused rather than quietly
    // served as VOLATILE.
    switch (qos->durability) {
    case NROS_RMW_DURABILITY_SYSTEM_DEFAULT:
    case NROS_RMW_DURABILITY_VOLATILE:
        break;
    default: // TRANSIENT_LOCAL, UNKNOWN
        return NROS_RMW_RET_INCOMPATIBLE_QOS;
    }

    // nros-qos-honours: HISTORY
    //
    // KEEP_ALL means "block rather than drop". The ring overwrites, so this
    // one cannot be granted down without losing exactly the samples the
    // caller asked to keep.
    if (qos->history == NROS_RMW_HISTORY_KEEP_ALL) {
        return NROS_RMW_RET_INCOMPATIBLE_QOS;
    }

    // nros-qos-honours: DEPTH — see `qos_granted` below, which is where the
    // requested `qos->depth` meets the ring the topic actually declares and
    // the grant is written back for the runtime to report.
    return NROS_RMW_RET_OK;
}

rmw_ret_t qos_granted(const struct orb_metadata* meta, rmw_qos_profile_t* in_out) {
    if (meta == nullptr || in_out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // issue 1437 — `in_out` arrives carrying `NROS_RMW_QOS_PROFILE_UNKNOWN`,
    // NOT the request. It carried the request until then, which inverted the
    // slot's own contract (`rmw_vtable.h`): a policy this backend cannot
    // report read back as GRANTED, so the one consumer saw agreement exactly
    // where nothing had been determined. Every field below is either WRITTEN
    // here — this backend determined it — or deliberately left as the
    // `*_UNKNOWN` it arrived as.
    //
    // nros-qos-honours: DEPTH
    //
    // `o_queue` is the ring the topic declares; a request for more than that
    // is granted down to it, and THIS is what makes the grant visible: the
    // runtime compares the answer against the request at create and reports
    // the difference (`report_qos_downgrade`). The `depth == 0` arm now covers
    // the UNKNOWN pre-load too, which carries depth 0 — upstream has no
    // "unknown depth" spelling, so the ring size is the answer either way.
    if (in_out->depth == 0 || in_out->depth > meta->o_queue) {
        in_out->depth = meta->o_queue;
    }
    // The ring is KEEP_LAST whatever was asked; `qos_admit` has already
    // refused KEEP_ALL.
    in_out->history = NROS_RMW_HISTORY_KEEP_LAST;
    // VOLATILE is the only durability `qos_admit` lets through, so an entity
    // that exists is running VOLATILE. Determinable without the request,
    // which is what lets it be reported.
    in_out->durability = NROS_RMW_DURABILITY_VOLATILE;
    // RELIABILITY is NOT written, and that is the honest answer rather than a
    // gap: both real values are served, so the grant IS the request — and the
    // entity state does not retain the request, so this backend genuinely
    // cannot say from here. `UNKNOWN` is what "cannot say" spells. It echoed
    // the request before issue 1437, which reported a confident grant on a
    // policy nothing had looked at.
    //
    // DEADLINE, LIFESPAN and LIVELINESS are not written for the same reason
    // one layer along: `qos_admit` refuses a stated value for each of them, so
    // there is no window in which this backend has an answer to report.
    return NROS_RMW_RET_OK;
}

rmw_ret_t supported_qos_policies(const rmw_session_t* /*session*/, uint32_t* out_mask) {
    if (out_mask == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Exactly the four `nros-qos-honours:` claims above. Deliberately NOT the
    // union the cffi route used to answer: deadline, lifespan, liveliness and
    // `avoid_ros_namespace_conventions` have no read site on this backend and
    // are refused at create rather than ignored after it.
    *out_mask = NROS_RMW_QOS_POLICY_CORE;
    return NROS_RMW_RET_OK;
}

} // namespace nros_rmw_uorb

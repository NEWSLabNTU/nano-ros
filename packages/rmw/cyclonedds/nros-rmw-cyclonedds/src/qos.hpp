#ifndef NROS_RMW_CYCLONEDDS_QOS_HPP
#define NROS_RMW_CYCLONEDDS_QOS_HPP

#include <dds/dds.h>

#include "nros/rmw_entity.h"

namespace nros_rmw_cyclonedds {

/**
 * Build a Cyclone `dds_qos_t` from an `rmw_qos_profile_t`. Caller owns
 * the returned pointer; release with `dds_delete_qos`. Returns
 * nullptr on allocation failure or null input.
 */
dds_qos_t *make_dds_qos(const rmw_qos_profile_t *src);

/**
 * Fold a Cyclone `dds_qos_t` into an `rmw_qos_profile_t` — the inverse of
 * `make_dds_qos`, and the ONE place that mapping lives.
 *
 * `out` must arrive carrying the profile the caller already believes: a policy
 * this `dds_qos_t` does not carry is left untouched, so an unreported field
 * reads as "unchanged" instead of as a zero that looks like an answer.
 *
 * Two callers, deliberately one function: `read_entity_qos` asks a LOCAL
 * entity (`dds_get_qos`), and the graph queries read a REMOTE endpoint's qos
 * straight off its `DCPSPublication` / `DCPSSubscription` sample. A second
 * transcription of this table is the hand-mirror class CLAUDE.md names.
 */
void qos_from_dds(const dds_qos_t *q, rmw_qos_profile_t *out);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_QOS_HPP

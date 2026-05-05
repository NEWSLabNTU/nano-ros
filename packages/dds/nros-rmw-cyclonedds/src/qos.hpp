#ifndef NROS_RMW_CYCLONEDDS_QOS_HPP
#define NROS_RMW_CYCLONEDDS_QOS_HPP

#include <dds/dds.h>

#include "nros/rmw_entity.h"

namespace nros_rmw_cyclonedds {

/**
 * Build a Cyclone `dds_qos_t` from an `nros_rmw_qos_t`. Caller owns
 * the returned pointer; release with `dds_delete_qos`. Returns
 * nullptr on allocation failure or null input.
 */
dds_qos_t *make_dds_qos(const nros_rmw_qos_t *src);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_QOS_HPP

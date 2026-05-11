// Publisher data plane.
//
// Phase 115.K.4.0 (scaffold): stubs return UNSUPPORTED. K.4.2 wires
// `orb_advertise_multi` in create, `orb_publish` in publish_raw, and
// `orb_unadvertise` in destroy.
//
// Topic-name → `orb_metadata *` resolution lives in K.4.3's topic
// registry (`topic_registry.cpp` / `.hpp`, landing alongside
// `publisher_create`'s real body).

#include "internal.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

nros_rmw_ret_t publisher_create(nros_rmw_session_t * /*session*/,
                                const char * /*topic_name*/,
                                const char * /*type_name*/,
                                const char * /*type_hash*/,
                                uint32_t /*domain_id*/,
                                const nros_rmw_qos_t * /*qos*/,
                                nros_rmw_publisher_t * /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

void publisher_destroy(nros_rmw_publisher_t * /*publisher*/) {
    // No-op until K.4.2 allocates per-publisher state.
}

nros_rmw_ret_t publisher_publish_raw(nros_rmw_publisher_t * /*publisher*/,
                                     const uint8_t * /*data*/,
                                     size_t /*len*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

} // namespace nros_rmw_uorb

// Subscriber data plane.
//
// Phase 115.K.4.0 (scaffold): stubs return UNSUPPORTED.
//
// K.4.2 plan:
// - create: `orb_subscribe_multi` + `orb_register_callback` that
//   signals a per-subscriber ringbuffer waker. The callback runs on
//   the PX4 workqueue context the orb was advertised on.
// - try_recv_raw: drain the ring (single-slot suffices on no-loss
//   uORB pubsub). `has_data` is the ring's ready flag.
// - destroy: `orb_unsubscribe` + free ring storage.

#include "internal.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

nros_rmw_ret_t subscriber_create(nros_rmw_session_t * /*session*/,
                                 const char * /*topic_name*/,
                                 const char * /*type_name*/,
                                 const char * /*type_hash*/,
                                 uint32_t /*domain_id*/,
                                 const nros_rmw_qos_t * /*qos*/,
                                 nros_rmw_subscriber_t * /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

void subscriber_destroy(nros_rmw_subscriber_t * /*subscriber*/) {
    // No-op until K.4.2 allocates per-subscriber state.
}

int32_t subscriber_try_recv_raw(nros_rmw_subscriber_t * /*subscriber*/,
                                uint8_t * /*buf*/, size_t /*buf_len*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

int32_t subscriber_has_data(nros_rmw_subscriber_t * /*subscriber*/) {
    return 0;
}

} // namespace nros_rmw_uorb

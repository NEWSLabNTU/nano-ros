// Service server/client.
//
// uORB has no native request/reply primitive. K.4.4 will decide
// between:
//   (a) service-over-topics with an in-payload correlator (mirrors
//       117.X.3's cdds_request_header_t shape for Cyclone) — ~400 LOC.
//   (b) Permanent UNSUPPORTED — acceptable for PX4 apps that stay
//       on pubsub-only patterns.
//
// Scaffold default is (b). All slots return UNSUPPORTED until K.4.4
// resolves the decision.

#include "internal.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

nros_rmw_ret_t service_create(nros_rmw_session_t* /*session*/, const char* /*service_name*/,
                                     const char* /*type_name*/, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const nros_rmw_qos_t* /*qos*/,
                                     nros_rmw_service_t* /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

void service_destroy(nros_rmw_service_t* /*server*/) {}

int32_t service_try_recv_request(nros_rmw_service_t* /*server*/, uint8_t* /*buf*/,
                                 size_t /*buf_len*/, int64_t* /*seq_out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

int32_t service_has_request(nros_rmw_service_t* /*server*/) {
    return 0;
}

nros_rmw_ret_t service_send_reply(nros_rmw_service_t* /*server*/, int64_t /*seq*/,
                                  const uint8_t* /*data*/, size_t /*len*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

nros_rmw_ret_t client_create(nros_rmw_session_t* /*session*/, const char* /*service_name*/,
                                     const char* /*type_name*/, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const nros_rmw_qos_t* /*qos*/,
                                     nros_rmw_client_t* /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

void client_destroy(nros_rmw_client_t* /*client*/) {}

// Phase-301: the deprecated blocking `call_raw` slot was deleted from
// the vtable; `send_request_raw` / `try_recv_reply_raw` stay NULL on
// this backend (services unsupported).

} // namespace nros_rmw_uorb

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

rmw_ret_t service_create(rmw_session_t* /*session*/, const char* /*service_name*/,
                                     const char* /*type_name*/, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const rmw_qos_profile_t* /*qos*/,
                                     rmw_service_t* /*out*/) {
    // Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
    // The node carries the route to its session (our `context`).
    if (node == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t service_destroy(rmw_service_t* /*server*/) {
    // uORB has no services; `create_service` never succeeds, so there is
    // nothing here that can fail.
    return NROS_RMW_RET_OK;
}

rmw_ret_t service_take_request(const rmw_service_t* /*server*/, uint8_t* /*buf*/,
                                    size_t /*buf_len*/, int64_t* /*seq_out*/,
                                    size_t* /*out_len*/, bool* /*taken*/) {
    /* uORB has no service transport — see service_send_reply. */
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t service_has_request(rmw_service_t* /*server*/, bool* out_has_request) {
    // uORB has no service transport at all — see `service_send_reply`, which
    // returns UNSUPPORTED. "Never a request" is the honest answer, not an error.
    if (out_has_request == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_has_request = false;
    return NROS_RMW_RET_OK;
}

rmw_ret_t service_send_reply(const rmw_service_t* /*server*/, int64_t /*seq*/,
                                  const uint8_t* /*data*/, size_t /*len*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t client_create(rmw_session_t* /*session*/, const char* /*service_name*/,
                                     const char* /*type_name*/, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const rmw_qos_profile_t* /*qos*/,
                                     rmw_client_t* /*out*/) {
    // Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
    // The node carries the route to its session (our `context`).
    if (node == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t client_destroy(rmw_client_t* /*client*/) {
    return NROS_RMW_RET_OK;
}

// Phase-301: the deprecated blocking `call_raw` slot was deleted from
// the vtable; `send_request_raw` / `try_recv_reply_raw` stay NULL on
// this backend (services unsupported).

} // namespace nros_rmw_uorb

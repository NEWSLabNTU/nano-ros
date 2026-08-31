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

// Phase 376 W5/B1 — takes the NODE, as upstream does. Every parameter is
// unused because uORB has no services at all: this returns UNSUPPORTED, and
// the slot exists so the runtime gets that answer instead of a NULL crash.
rmw_ret_t service_create(const rmw_node_t* /*node*/,
                                     const rmw_service_type_support_t* /*type_support*/,
                                     const char* /*service_name*/,
                                     uint32_t /*domain_id*/, const rmw_qos_profile_t* /*qos*/,
                                     rmw_service_t* /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t service_destroy(rmw_service_t* /*server*/) {
    // uORB has no services; `create_service` never succeeds, so there is
    // nothing here that can fail.
    return NROS_RMW_RET_OK;
}

rmw_ret_t service_take_request(const rmw_service_t* /*server*/,
                                    rmw_mut_byte_span_t* /*request*/, int64_t* /*seq_out*/,
                                    bool* /*taken*/) {
    /* uORB has no service transport — see service_send_response. */
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t service_has_request(rmw_service_t* /*server*/, bool* out_has_request) {
    // uORB has no service transport at all — see `service_send_response`, which
    // returns UNSUPPORTED. "Never a request" is the honest answer, not an error.
    if (out_has_request == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_has_request = false;
    return NROS_RMW_RET_OK;
}

rmw_ret_t service_send_response(const rmw_service_t* /*server*/, int64_t /*seq*/,
                                  rmw_byte_span_t /*response*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

// Phase 376 W5/B1 — takes the NODE, as upstream does. Every parameter is
// unused because uORB has no services at all: this returns UNSUPPORTED, and
// the slot exists so the runtime gets that answer instead of a NULL crash.
rmw_ret_t client_create(const rmw_node_t* /*node*/,
                                     const rmw_service_type_support_t* /*type_support*/,
                                     const char* /*service_name*/,
                                     uint32_t /*domain_id*/, const rmw_qos_profile_t* /*qos*/,
                                     rmw_client_t* /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

rmw_ret_t client_destroy(rmw_client_t* /*client*/) {
    return NROS_RMW_RET_OK;
}

// Phase-301: the deprecated blocking `call_raw` slot was deleted from
// the vtable; `send_request_raw` / `try_recv_reply_raw` stay NULL on
// this backend (services unsupported).

} // namespace nros_rmw_uorb

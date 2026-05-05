// Cyclone DDS RMW backend — vtable assembly + register entry point.
//
// Phase 117.3: every slot points at the matching stub function in
// session.cpp / publisher.cpp / subscriber.cpp / service.cpp. Stubs
// return NROS_RMW_RET_UNSUPPORTED so the runtime sees a wired-but-
// inert backend until 117.4–117.7 fill them in.

#include "nros_rmw_cyclonedds.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.hpp"

namespace {

using namespace nros_rmw_cyclonedds;

// Phase 108 event hooks left NULL until a follow-up phase wires
// Cyclone listeners through to the runtime's status-event surface.
constexpr nros_rmw_ret_t (*kRegisterSubscriberEvent)(
    nros_rmw_subscriber_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kRegisterPublisherEvent)(
    nros_rmw_publisher_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kAssertPublisherLiveliness)(
    nros_rmw_publisher_t *) = nullptr;

const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*open*/                      session_open,
    /*close*/                     session_close,
    /*drive_io*/                  session_drive_io,

    /* ---- Publisher ---- */
    /*create_publisher*/          publisher_create,
    /*destroy_publisher*/         publisher_destroy,
    /*publish_raw*/               publisher_publish_raw,

    /* ---- Subscriber ---- */
    /*create_subscriber*/         subscriber_create,
    /*destroy_subscriber*/        subscriber_destroy,
    /*try_recv_raw*/              subscriber_try_recv_raw,
    /*has_data*/                  subscriber_has_data,

    /* ---- Service Server ---- */
    /*create_service_server*/     service_server_create,
    /*destroy_service_server*/    service_server_destroy,
    /*try_recv_request*/          service_try_recv_request,
    /*has_request*/               service_has_request,
    /*send_reply*/                service_send_reply,

    /* ---- Service Client ---- */
    /*create_service_client*/     service_client_create,
    /*destroy_service_client*/    service_client_destroy,
    /*call_raw*/                  service_call_raw,

    /* ---- Phase 108 event hooks (deferred) ---- */
    /*register_subscriber_event*/ kRegisterSubscriberEvent,
    /*register_publisher_event*/  kRegisterPublisherEvent,
    /*assert_publisher_liveliness*/ kAssertPublisherLiveliness,
};

} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cyclonedds_register(void) {
    return nros_rmw_cffi_register(&kVtable);
}

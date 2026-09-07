/* See stub_rmw_backend.h for why this exists. */

#include "stub_rmw_backend.h"

#include <nros/rmw_vtable.h>

#include <stddef.h>
#include <time.h>

static int s_backend_data = 0;
static uint32_t s_drive_io_calls = 0;

/* ---- Session lifecycle: the only slots that succeed ---------------------- */

static rmw_ret_t stub_create_session(const char* locator, uint8_t mode, uint32_t domain_id,
                                     const char* node_name, const rmw_session_options_t* options,
                                     rmw_session_t* out) {
    (void)locator;
    (void)mode;
    (void)domain_id;
    (void)node_name;
    (void)options;
    /* The runtime treats a NULL `backend_data` as "no session"; anything
     * non-NULL and stable will do, and nothing dereferences it here. */
    out->backend_data = &s_backend_data;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_destroy_session(rmw_session_t* session) {
    (void)session;
    return NROS_RMW_RET_OK;
}

/* Honours the timeout by sleeping it out, which is what a real backend's
 * blocking read does and is what makes WALL TIME pass in a spin loop. A stub
 * that returned instantly would make every steady-clock timer in a probe look
 * like it never fires — the executor's spin delta is real time, and a loop
 * over a non-blocking drive_io does not spend any. */
static rmw_ret_t stub_drive_io(rmw_session_t* session, int32_t timeout_ms) {
    (void)session;
    s_drive_io_calls++;
    if (timeout_ms > 0) {
        struct timespec req;
        req.tv_sec = (time_t)(timeout_ms / 1000);
        req.tv_nsec = (long)(timeout_ms % 1000) * 1000000L;
        (void)nanosleep(&req, NULL);
    }
    return NROS_RMW_RET_OK;
}

/* ---- Entities: present so registration succeeds, refusing so a test that
 *      strays past this backend's purpose fails loudly ---------------------- */

static rmw_ret_t
stub_create_publisher(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                      const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                      const rmw_publisher_options_t* options, rmw_publisher_t* out) {
    (void)node;
    (void)type_support;
    (void)topic_name;
    (void)domain_id;
    (void)qos;
    (void)options;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_publisher(rmw_publisher_t* publisher) {
    (void)publisher;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_publish(const rmw_publisher_t* publisher, rmw_byte_span_t payload) {
    (void)publisher;
    (void)payload;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t
stub_create_subscription(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                         const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                         const rmw_subscription_options_t* options, rmw_subscription_t* out) {
    (void)node;
    (void)type_support;
    (void)topic_name;
    (void)domain_id;
    (void)qos;
    (void)options;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_subscription(rmw_subscription_t* subscription) {
    (void)subscription;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take(const rmw_subscription_t* subscription, rmw_mut_byte_span_t* out,
                           bool* taken) {
    (void)subscription;
    (void)out;
    if (taken != NULL) {
        *taken = false;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_has_data(rmw_subscription_t* subscription, bool* out_has_data) {
    (void)subscription;
    if (out_has_data != NULL) {
        *out_has_data = false;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_create_service(const rmw_node_t* node,
                                     const rmw_service_type_support_t* type_support,
                                     const char* service_name, uint32_t domain_id,
                                     const rmw_qos_profile_t* qos, rmw_service_t* out) {
    (void)node;
    (void)type_support;
    (void)service_name;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_service(rmw_service_t* server) {
    (void)server;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take_request(const rmw_service_t* server, rmw_mut_byte_span_t* request,
                                   int64_t* seq_out, bool* taken) {
    (void)server;
    (void)request;
    (void)seq_out;
    if (taken != NULL) {
        *taken = false;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_has_request(rmw_service_t* server, bool* out_has_request) {
    (void)server;
    if (out_has_request != NULL) {
        *out_has_request = false;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_send_response(const rmw_service_t* server, int64_t seq,
                                    rmw_byte_span_t response) {
    (void)server;
    (void)seq;
    (void)response;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_create_client(const rmw_node_t* node,
                                    const rmw_service_type_support_t* type_support,
                                    const char* service_name, uint32_t domain_id,
                                    const rmw_qos_profile_t* qos, rmw_client_t* out) {
    (void)node;
    (void)type_support;
    (void)service_name;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_client(rmw_client_t* client) {
    (void)client;
    return NROS_RMW_RET_UNSUPPORTED;
}

static const nros_rmw_vtable_t STUB_VTABLE = {
    .create_session = stub_create_session,
    .destroy_session = stub_destroy_session,
    .drive_io = stub_drive_io,
    .create_publisher = stub_create_publisher,
    .destroy_publisher = stub_destroy_publisher,
    .publish = stub_publish,
    .create_subscription = stub_create_subscription,
    .destroy_subscription = stub_destroy_subscription,
    .take = stub_take,
    .has_data = stub_has_data,
    .create_service = stub_create_service,
    .destroy_service = stub_destroy_service,
    .take_request = stub_take_request,
    .has_request = stub_has_request,
    .send_response = stub_send_response,
    .create_client = stub_create_client,
    .destroy_client = stub_destroy_client,
};

int32_t nros_stub_rmw_register(void) {
    return (int32_t)nros_rmw_cffi_register_named(NROS_STUB_RMW_NAME, &STUB_VTABLE);
}

uint32_t nros_stub_rmw_drive_io_calls(void) {
    return s_drive_io_calls;
}

/// @file AddServer.c
/// @brief pure-C workspace — A1 services: the AddTwoInts service SERVER, typed component
/// (RFC-0043 / phase-257). The C projection of the Rust `service_server_pkg`.
///
/// `add_server_configure` registers a raw callback-style service server on `/add_two_ints`
/// via `nros_cpp_service_server_register` (the executor-scoped component seam). The callback
/// deserializes the request, computes `a + b`, and serializes the reply — lifted verbatim from
/// the proven `examples/native/c/service-server`, but wired through the component factory the
/// typed C Entry drives (`nros_board_native_run_components`), not a standalone `main`.
///
/// Cross-process only (issue 0096): an in-process (same-executor) server+client can't talk, so
/// the server runs as its OWN entry (`native_service_server_entry`) and the client as another.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
// (nros_find_interfaces(LANGUAGE C) → example_interfaces__nano_ros_c).
#include "example_interfaces.h"

typedef struct {
    int request_count;
} add_server_t;

/// Raw callback-style service handler — receives the request CDR, fills the reply CDR.
/// ABI-identical to the native service-server's `service_callback`.
static bool add_server_callback(const uint8_t* req_data, size_t req_len, uint8_t* resp_data,
                                size_t resp_cap, size_t* resp_len, void* ctx) {
    add_server_t* self = (add_server_t*)ctx;

    example_interfaces_srv_add_two_ints_request req;
    if (example_interfaces_srv_add_two_ints_request_deserialize(&req, req_data, req_len) != 0) {
        return false;
    }

    self->request_count++;

    example_interfaces_srv_add_two_ints_response resp;
    example_interfaces_srv_add_two_ints_response_init(&resp);
    resp.sum = req.a + req.b;

    printf("[c_add_server_pkg] %lld + %lld = %lld\n", (long long)req.a, (long long)req.b,
           (long long)resp.sum);

    size_t len = 0;
    int32_t len_rc =
        example_interfaces_srv_add_two_ints_response_serialize(&resp, resp_data, resp_cap, &len);
    if (len_rc != 0) {
        return false;
    }
    *resp_len = len;
    return true;
}

static nros_ret_t add_server_configure(const nros_cpp_node_t* node, void* executor,
                                       add_server_t* self) {
    (void)executor; /* service server is node-scoped; executor unused */
    /* Line-buffer stdout so each line flushes when piped to the test harness. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->request_count = 0;

    size_t handle;
    int32_t rc = nros_cpp_service_server_register(
        node, "/add_two_ints", example_interfaces_srv_add_two_ints_get_type_name(),
        example_interfaces_srv_add_two_ints_get_type_hash(), nros_c_qos_default(),
        add_server_callback, self, /*sched_context=*/0, &handle);
    if (rc == 0) {
        printf("[c_add_server_pkg] add_two_ints server ready\n");
    }
    return rc;
}

NROS_C_COMPONENT(add_server_t, add_server_configure)

// Phase 117.7 service entity-plumbing smoke test.
//
// Verifies service_server_create / service_client_create succeed
// when both `<svc>_Request` and `<svc>_Response` descriptors are
// registered, fail cleanly with UNSUPPORTED when they aren't.
// Data plane stubs (`try_recv_request` / `send_response` /
// `send_request_raw`) are still UNSUPPORTED until the raw-CDR
// follow-up lands.

#include <cstdio>
#include <cstring>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char * /*name*/,
                                                        const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    rmw_session_t s{};
    s.node_name  = "service_smoke";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    // Phase 376 W5/B1 — entities are created ON A NODE now. The node
    // carries its own identity plus the route to its session.
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_service_t srv{};
    srv.service_name = "add_two_ints";
    srv.type_name    = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_1{srv.type_name, ""};
    if (g_vt->create_service(&node, &ts_1, srv.service_name, 99, nullptr, &srv) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_service failed\n");
        (void) g_vt->destroy_session(&s);
        return 3;
    }
    if (srv.backend_data == nullptr) {
        std::fprintf(stderr, "server backend_data NULL\n");
        return 4;
    }

    rmw_client_t cli{};
    cli.service_name = "add_two_ints";
    cli.type_name    = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_2{cli.type_name, ""};
    if (g_vt->create_client(&node, &ts_2, cli.service_name, 99, nullptr, &cli) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_client failed\n");
        g_vt->destroy_service(&srv);
        (void) g_vt->destroy_session(&s);
        return 5;
    }

    // No traffic yet — has_request must report false, with an OK status.
    bool has_r = true;
    if (g_vt->has_request(&srv, &has_r) != NROS_RMW_RET_OK || has_r) {
        std::fprintf(stderr, "has_request should be false with no traffic\n");
        return 6;
    }
    // send_request_raw with too-short request → invalid arg.
    if (g_vt->send_request(&cli, rmw_byte_span_t{reinterpret_cast<const uint8_t *>("x"), 1}, nullptr)
        != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "send_request_raw too-short should be INVALID_ARGUMENT\n");
        return 7;
    }

    // Phase 117.X.3: per-service typed-IDL registry is required.
    // An unregistered type name must be rejected with UNSUPPORTED
    // so consumers get a clear error if they forgot to call the
    // codegen helper.
    rmw_service_t any{};
    const rmw_service_type_support_t ts_3{"no::such::Svc", ""};
    if (g_vt->create_service(&node, &ts_3, "missing", 99, nullptr, &any) != NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr,
            "missing type_name should report UNSUPPORTED\n");
        return 8;
    }

    g_vt->destroy_client(&cli);
    g_vt->destroy_service(&srv);
    (void) g_vt->destroy_session(&s);
    std::printf("OK\n");
    return 0;
}

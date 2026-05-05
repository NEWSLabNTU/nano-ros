// Phase 117.7 service entity-plumbing smoke test.
//
// Verifies service_server_create / service_client_create succeed
// when both `<svc>_Request` and `<svc>_Response` descriptors are
// registered, fail cleanly with UNSUPPORTED when they aren't.
// Data plane stubs (`try_recv_request` / `send_reply` / `call_raw`)
// are still UNSUPPORTED until the raw-CDR follow-up lands.

#include <cstdio>
#include <cstring>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "service_smoke";
    s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 99, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_service_server_t srv{};
    srv.service_name = "add_two_ints";
    srv.type_name    = "nros_test::srv::AddTwoInts";
    if (g_vt->create_service_server(&s, srv.service_name, srv.type_name, "",
                                    99, &srv) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_service_server failed\n");
        (void) g_vt->close(&s);
        return 3;
    }
    if (srv.backend_data == nullptr) {
        std::fprintf(stderr, "server backend_data NULL\n");
        return 4;
    }

    nros_rmw_service_client_t cli{};
    cli.service_name = "add_two_ints";
    cli.type_name    = "nros_test::srv::AddTwoInts";
    if (g_vt->create_service_client(&s, cli.service_name, cli.type_name, "",
                                    99, &cli) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_service_client failed\n");
        g_vt->destroy_service_server(&srv);
        (void) g_vt->close(&s);
        return 5;
    }

    // No traffic yet — has_request must be 0.
    if (g_vt->has_request(&srv) != 0) {
        std::fprintf(stderr, "has_request should be 0 with no traffic\n");
        return 6;
    }
    // call_raw with too-short request → invalid arg.
    if (g_vt->call_raw(&cli,
            reinterpret_cast<const uint8_t *>("x"), 1, nullptr, 0)
        != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "call_raw too-short should be INVALID_ARGUMENT\n");
        return 7;
    }

    // Missing user type is now NO LONGER an error: Phase 117.7.B
    // wraps every service in a backend-defined envelope topic that
    // doesn't depend on the user IDL. Verify the create succeeds
    // and clean up.
    nros_rmw_service_server_t any{};
    if (g_vt->create_service_server(&s, "rq/anything", "no::such::Svc", "",
                                    99, &any) != NROS_RMW_RET_OK) {
        std::fprintf(stderr,
            "envelope-based services should accept any type_name\n");
        return 8;
    }
    g_vt->destroy_service_server(&any);

    g_vt->destroy_service_client(&cli);
    g_vt->destroy_service_server(&srv);
    (void) g_vt->close(&s);
    std::printf("OK\n");
    return 0;
}

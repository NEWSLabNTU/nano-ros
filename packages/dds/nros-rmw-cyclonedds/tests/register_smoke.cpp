// Phase 117.3 smoke test: link the backend, call its register entry
// point, assert it returns NROS_RMW_RET_OK.
//
// `nros_rmw_cffi_register` is provided as a stub here (the runtime
// hasn't been pulled into this build context). The stub captures the
// vtable pointer so the test can sanity-check that none of the
// mandatory function pointer slots are NULL.
//
// 117.12 swaps this for a real interop test against
// `nros-rmw-cffi`'s real `nros_rmw_cffi_register`.

#include <cstdio>
#include <cstdlib>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {

const nros_rmw_vtable_t *g_captured = nullptr;

} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_captured = vtable;
    return NROS_RMW_RET_OK;
}

int main() {
    nros_rmw_ret_t r = nros_rmw_cyclonedds_register();
    if (r != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "register returned %d, expected 0\n", static_cast<int>(r));
        return 1;
    }
    if (g_captured == nullptr) {
        std::fprintf(stderr, "vtable not captured\n");
        return 2;
    }
    // Mandatory slots: session lifecycle + pub/sub + service. Phase 108
    // event hooks are allowed NULL.
    if (g_captured->open == nullptr ||
        g_captured->close == nullptr ||
        g_captured->drive_io == nullptr ||
        g_captured->create_publisher == nullptr ||
        g_captured->destroy_publisher == nullptr ||
        g_captured->publish_raw == nullptr ||
        g_captured->create_subscriber == nullptr ||
        g_captured->destroy_subscriber == nullptr ||
        g_captured->try_recv_raw == nullptr ||
        g_captured->has_data == nullptr ||
        g_captured->create_service_server == nullptr ||
        g_captured->destroy_service_server == nullptr ||
        g_captured->try_recv_request == nullptr ||
        g_captured->has_request == nullptr ||
        g_captured->send_reply == nullptr ||
        g_captured->create_service_client == nullptr ||
        g_captured->destroy_service_client == nullptr ||
        g_captured->call_raw == nullptr) {
        std::fprintf(stderr, "vtable has NULL mandatory slot\n");
        return 3;
    }
    std::printf("OK\n");
    return 0;
}

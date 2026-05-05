// Phase 117.4 smoke test: session_open creates a Cyclone domain
// participant, session_close tears it down. Round-trips through the
// real vtable so we exercise the same path the runtime will use.
//
// Stubs `nros_rmw_cffi_register` (the runtime is not linked here) and
// drives the captured vtable directly.

#include <cstdio>
#include <cstring>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {

const nros_rmw_vtable_t *g_vt = nullptr;

} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_vt = vtable;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "nros_rmw_cyclonedds_session_smoke";
    s.namespace_ = "/";

    // Domain 42 keeps this test off the default ROS_DOMAIN_ID so a
    // running ROS 2 stack on the same host doesn't see our short-
    // lived participant.
    nros_rmw_ret_t r = g_vt->open(nullptr, 0, 42, s.node_name, &s);
    if (r != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open returned %d\n", static_cast<int>(r));
        return 2;
    }
    if (s.backend_data == nullptr) {
        std::fprintf(stderr, "backend_data is NULL after open\n");
        return 3;
    }

    // drive_io is a no-op for Cyclone; just exercise the slot.
    if (g_vt->drive_io(&s, 0) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "drive_io failed\n");
        return 4;
    }

    if (g_vt->close(&s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close failed\n");
        return 5;
    }
    if (s.backend_data != nullptr) {
        std::fprintf(stderr, "backend_data not cleared by close\n");
        return 6;
    }

    std::printf("OK\n");
    return 0;
}

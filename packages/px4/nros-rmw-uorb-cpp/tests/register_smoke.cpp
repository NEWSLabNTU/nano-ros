// Phase 115.K.4.0 — smoke test for nros_rmw_uorb_register().
//
// Stubs `nros_rmw_cffi_register` locally so the test driver compiles
// without linking the Rust-side staticlib that owns the real symbol.
// The stub stashes the vtable pointer it received and confirms a few
// fn slots are non-null; once K.4.1 wires session_open / close,
// extend the test to call those slots through the stashed vtable
// pointer.

#include <cstdio>
#include <cstdlib>

#include "nros_rmw_uorb.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

namespace {
const nros_rmw_vtable_t *g_stashed_vtable = nullptr;
}

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_stashed_vtable = vtable;
    return NROS_RMW_RET_OK;
}

int main() {
    nros_rmw_ret_t rc = nros_rmw_uorb_register();
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "register returned %d, expected 0\n", rc);
        return 1;
    }
    if (g_stashed_vtable == nullptr) {
        std::fprintf(stderr, "vtable pointer not stashed\n");
        return 1;
    }
    // Spot-check: a few slots must be non-null. NULL is reserved for
    // optional event hooks; the lifecycle / data-plane slots must
    // resolve to real (stub-returning-UNSUPPORTED) functions.
    const auto *vt = g_stashed_vtable;
    if (vt->open == nullptr || vt->close == nullptr
        || vt->create_publisher == nullptr
        || vt->create_subscriber == nullptr) {
        std::fprintf(stderr, "required vtable slot is NULL\n");
        return 1;
    }
    // K.4.1 — open + close round-trip. uORB ignores the locator,
    // session mode, and domain id; the only validated state is
    // `backend_data` allocated by open and freed by close.
    nros_rmw_session_t session{};
    rc = vt->open("/* ignored */", 0, 0, "test_module", &session);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open returned %d, expected OK\n", rc);
        return 1;
    }
    if (session.backend_data == nullptr) {
        std::fprintf(stderr, "open did not populate backend_data\n");
        return 1;
    }
    // drive_io is a no-op for uORB (push-based delivery).
    rc = vt->drive_io(&session, 0);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "drive_io returned %d, expected OK\n", rc);
        return 1;
    }
    rc = vt->close(&session);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close returned %d, expected OK\n", rc);
        return 1;
    }
    if (session.backend_data != nullptr) {
        std::fprintf(stderr, "close did not clear backend_data\n");
        return 1;
    }

    // K.4.2 / K.4.3 / K.4.4 still UNSUPPORTED — spot check.
    nros_rmw_publisher_t pubp{};
    rc = vt->create_publisher(&session, "/t", "T", "H", 0, nullptr, &pubp);
    if (rc != NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr, "create_publisher returned %d, expected UNSUPPORTED\n", rc);
        return 1;
    }

    // Null-arg rejection on open.
    rc = vt->open(nullptr, 0, 0, nullptr, nullptr);
    if (rc != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "open(null out) returned %d, expected INVALID_ARGUMENT\n", rc);
        return 1;
    }

    std::printf("[OK] nros_rmw_uorb K.4.1 session lifecycle passes\n");
    return 0;
}

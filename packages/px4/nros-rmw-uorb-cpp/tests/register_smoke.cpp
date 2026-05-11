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
    // And confirm a stub returns UNSUPPORTED — this is what K.4.0
    // documents as the scaffold contract.
    rc = vt->open(nullptr, 0, 0, nullptr, nullptr);
    if (rc != NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr, "open returned %d, expected UNSUPPORTED\n", rc);
        return 1;
    }
    std::printf("[OK] nros_rmw_uorb K.4.0 scaffold passes\n");
    return 0;
}

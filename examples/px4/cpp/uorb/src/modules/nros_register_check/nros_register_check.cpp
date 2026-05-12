// Phase 115.K.4.5 — PX4-SITL register-check module main.
//
// Trivial PX4 module: on launch, call `nros_rmw_uorb_register()` and
// log the return code. The build itself is the validation; the
// runtime printf only matters when somebody pokes the module from the
// PX4 shell (`nros_register_check start` in pxh).
//
// The full nano-ros runtime ships `nros_rmw_cffi_register` from the
// Rust-side `nros-rmw-cffi` crate (linked via the workspace's
// `staticlib` artifact). The SITL build path doesn't drag cargo in
// — that's a Phase 115.L concern — so we stub the registry with a
// minimal in-module implementation that accepts the vtable and
// returns OK. This keeps the K.4.5 validation honest about the C++
// half (compile + link inside `px4_add_module()`) without spilling
// over into the L-tier Rust integration story.

#include "nros_rmw_uorb.h"
#include "nros/rmw_vtable.h"

#include <px4_platform_common/log.h>

extern "C" {

// SITL-local stub of the runtime registry. Real builds replace this
// with the Rust-side strong symbol from `nros-rmw-cffi`.
__attribute__((weak)) nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable)
{
    if (vtable == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return NROS_RMW_RET_OK;
}

__EXPORT int nros_register_check_main(int argc, char *argv[]);

__EXPORT int nros_register_check_main(int /*argc*/, char * /*argv*/[])
{
    nros_rmw_ret_t rc = nros_rmw_uorb_register();
    if (rc == NROS_RMW_RET_OK) {
        PX4_INFO("nros_rmw_uorb_register() -> OK");
        return 0;
    }
    PX4_ERR("nros_rmw_uorb_register() -> %d", static_cast<int>(rc));
    return 1;
}

} // extern "C"

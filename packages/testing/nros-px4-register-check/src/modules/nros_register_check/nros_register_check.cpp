// Phase 115.K.4.5 — PX4-SITL register-check module main.
//
// Trivial PX4 module: on launch, call `nros_rmw_uorb_register()` and
// log the return code. The build itself is the validation; the
// runtime printf only matters when somebody pokes the module from the
// PX4 shell (`nros_register_check start` in pxh).
//
// phase-244 D5 — the SITL-only weak `nros_rmw_cffi_register` link stub moved
// out to the `sitl_register_stub.c` build scaffold (the registry symbol is
// build wiring, not application logic). This TU now carries only the check.

#include "nros_rmw_uorb.h"

#include <px4_platform_common/log.h>

extern "C" {

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

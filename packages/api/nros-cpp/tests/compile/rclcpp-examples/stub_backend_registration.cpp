// phase-488 — the one line a ported rclcpp example cannot carry itself.
//
// `nros_app_register_backends` is the image's backend registration, which a
// real build gets from its generated entry or from `NanoRos::NanoRos`'s link
// of the selected RMW. These fixtures link the stub RMW backend
// (`nros-c/tests/run/stub_rmw_backend.c`) instead, so they boot, open a
// session and create every entity with no router and no wire. Kept in its own
// TU so the ported example sources stay exactly what `edits.json` counts.

extern "C" {
#include "stub_rmw_backend.h"
}

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    // Accept mode: every create succeeds, so the examples reach their spin.
    nros_stub_rmw_set_accept_entities(true);
    (void)nros_stub_rmw_register();
}

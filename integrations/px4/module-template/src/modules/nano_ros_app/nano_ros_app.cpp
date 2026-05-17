// Phase 139.5 — PX4 module template entry point.
//
// Minimal main showing how a PX4 module wires up nano-ros: call the
// RMW backend register hook on launch, then drop into a per-tick
// publisher loop using the C++ API. The body is intentionally
// sparse — downstream copies replace the "publisher loop" comment
// with their topic / message logic.
//
// Building this file outside a real PX4 + nano-ros workspace will
// fail: it depends on PX4's px4_platform_common headers AND the
// nano-ros C++ headers. The template is meant to be COPIED into a
// downstream tree, not built in-place. See integrations/px4/README.md.

#include <nros/init.h>
#include <px4_platform_common/log.h>

extern "C" {

// Forward decl — real builds resolve via the Rust-side
// nros-rmw-cffi crate (see examples/px4/cpp/uorb/ for the
// SITL-validated weak-stub pattern).
extern int nros_rmw_uorb_register(void);

__EXPORT int nano_ros_app_main(int argc, char *argv[]);

__EXPORT int nano_ros_app_main(int /*argc*/, char * /*argv*/[])
{
    // 1. Register the uORB-backed RMW backend.
    int rc = nros_rmw_uorb_register();
    if (rc != 0) {
        PX4_ERR("nros_rmw_uorb_register() -> %d", rc);
        return 1;
    }
    PX4_INFO("nano-ros uORB backend registered");

    // 2. Init nano-ros support and run a publisher loop.
    //    Replace this comment block with NodeBuilder / Publisher
    //    calls from the C++ API; see book/src/reference/cpp-api.md.
    //
    // nros_support_t support = nros_support_get_zero_initialized();
    // ...

    return 0;
}

} // extern "C"

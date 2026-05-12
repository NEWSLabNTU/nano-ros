# PX4 EXTERNAL_MODULES_LOCATION entry point for the nano-ros uORB C++
# integration test modules.
#
# Pass this directory's parent (`examples/px4/cpp/uorb/`) as
# `EXTERNAL_MODULES_LOCATION` to PX4's `make`. PX4 looks at
# `<location>/src/CMakeLists.txt` (this file) and pulls in each
# subdirectory listed in `config_module_list_external`.
#
# Today this tree carries a single module — `nros_register_check` —
# which is the Phase 115.K.4.5 validation gate: it links the
# `nros-rmw-uorb-cpp` static library (including the
# `SubscriptionCallbackWorkItem` push-wake glue) inside a real PX4
# SITL build and confirms `nros_rmw_uorb_register()` succeeds.

set(config_module_list_external
    modules/nros_register_check
    PARENT_SCOPE
)

# PX4 EXTERNAL_MODULES_LOCATION entry point for the nano-ros uORB interop example.
#
# Pass this directory's parent (`examples/px4/cpp/firmware/`) as
# EXTERNAL_MODULES_LOCATION. PX4 looks at `<location>/src/CMakeLists.txt` (this
# file) and pulls in each subdirectory named in `config_module_list_external`.

set(config_module_list_external
    modules/nros_uorb_demo
    PARENT_SCOPE
)

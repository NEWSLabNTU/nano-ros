# Phase 139.5 — PX4 EXTERNAL_MODULES_LOCATION entry.
#
# PX4 reads `<location>/src/CMakeLists.txt` to discover external
# modules. Each name listed here resolves to `<location>/src/modules/<name>/`.
#
# Adjust this list when adding modules to the template. Default
# carries a single example: `nano_ros_app`.

set(config_module_list_external
    modules/nano_ros_app
    PARENT_SCOPE
)

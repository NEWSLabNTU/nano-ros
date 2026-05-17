# Phase 139.1 — Zephyr integration shell.
#
# Wires the Phase 137 / 138 root nano-ros CMake into the Zephyr build
# under west's module discovery. Activation is gated by `CONFIG_NROS`
# (see Kconfig sibling) so the module is inert when the user hasn't
# enabled it in their `prj.conf`.
#
# Contract:
#   - Sets NANO_ROS_PLATFORM=zephyr + NANO_ROS_RMW=<CONFIG_NROS_RMW>
#     in the cache BEFORE pulling in the root CMakeLists.
#   - Re-exports the umbrella `NanoRos::NanoRos` target to Zephyr's
#     `app` library.
#
# At ≤50 lines (excl. comments) per Phase 139's per-shell budget.

if(CONFIG_NROS)
    # Repository root sits two levels up from `integrations/zephyr/`.
    set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../..")

    # Cache vars must land BEFORE the root `project(NanoRos ...)` call.
    set(NANO_ROS_PLATFORM "zephyr" CACHE STRING "" FORCE)
    set(NANO_ROS_RMW         "${CONFIG_NROS_RMW}"         CACHE STRING "" FORCE)
    set(NANO_ROS_ROS_EDITION "${CONFIG_NROS_ROS_EDITION}" CACHE STRING "" FORCE)

    # Pull in the unified root CMake. Use a unique sub-build dir name so
    # multiple shells (zephyr + esp-idf + ...) in the same superproject
    # don't collide.
    add_subdirectory("${_nros_root}" nano_ros_root)

    # Surface the umbrella target on Zephyr's `app` library so user code
    # links transparently. `zephyr_library_*` mutates the most-recently-
    # declared library, so name a fresh one to scope the link.
    zephyr_library_named(nano_ros_integration)
    zephyr_library_link_libraries(NanoRos::NanoRos)
endif()

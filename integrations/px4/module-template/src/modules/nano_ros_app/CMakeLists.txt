# Phase 139.5 — PX4 module CMakeLists template (copy-out shape).
#
# Adapt this file for your PX4 module:
#   - Set MAIN to your module entry-point name (matches the C++
#     <name>_main symbol).
#   - List your .cpp sources under SRCS.
#   - Add any extra includes / link deps.
#
# The module links against the nano-ros C++ headers. Real PX4
# integration of the Rust-built libnros_cpp.a goes through the
# `nros-rmw-cffi_register` shim — see examples/px4/cpp/uorb/ for the
# end-to-end pattern (Phase 115.K.4.5).

set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../../../../..")
# Allow downstream copies to override the path (when the template
# sits in a separate tree from nano-ros). Phase 208.D.11 — accept
# both a cache var (`-DNANO_ROS_DIR=<path>`) and an env var; the
# cache form takes precedence so an explicit configure-line override
# wins, then env, then the in-tree default.
if(DEFINED NANO_ROS_DIR)
    set(_nros_root "${NANO_ROS_DIR}")
elseif(DEFINED ENV{NANO_ROS_DIR})
    set(_nros_root "$ENV{NANO_ROS_DIR}")
endif()

px4_add_module(
    MODULE         modules__nano_ros_app
    MAIN           nano_ros_app
    INCLUDES
        ${_nros_root}/packages/core/nros-cpp/include
        ${_nros_root}/packages/core/nros-c/include
        ${_nros_root}/packages/core/nros-rmw-abi/include
    SRCS
        ${CMAKE_CURRENT_LIST_DIR}/nano_ros_app.cpp
)

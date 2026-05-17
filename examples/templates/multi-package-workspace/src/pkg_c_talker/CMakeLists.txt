cmake_minimum_required(VERSION 3.20)
project(pkg_c_talker LANGUAGES C)

# Phase 123.A.10 — multi-package workspace demo.
#
# Tiny C talker that:
#   * Generates std_msgs C bindings via nros_generate_interfaces().
#   * Links the nano-ros platform + RMW backends via the
#     nano_ros_link_platform / nano_ros_link_rmw helpers
#     (Phase 123.A.1.x.5).
#
# `find_package(NanoRos)` locates the installed nano-ros prefix
# (cmake -DCMAKE_PREFIX_PATH=<install-dir>).
find_package(NanoRos REQUIRED CONFIG)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(pkg_c_talker src/main.c)
# Phase 123.A.10 — call RMW link helper BEFORE platform so that
# at user-target level the link order ends with `... rmw platform`.
# libnros_rmw_<rmw>.a's undefined `nros_platform_*` refs need
# `libnros_platform_<plat>.a` to follow in single-pass ld scanning.
nano_ros_link_rmw(pkg_c_talker)
nano_ros_link_platform(pkg_c_talker)
target_link_libraries(pkg_c_talker PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)

install(TARGETS pkg_c_talker RUNTIME DESTINATION lib/${PROJECT_NAME})

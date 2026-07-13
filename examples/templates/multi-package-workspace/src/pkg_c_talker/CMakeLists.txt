cmake_minimum_required(VERSION 3.22)
# C CXX: the node logic is a C typed component (src/Talker.c), but the generated
# native typed-entry carrier is C++ (drives NativeBoard::run_components).
project(pkg_c_talker LANGUAGES C CXX)

# Phase 140 / 244.C4 — multi-package workspace demo, add_subdirectory shape.
#
# C talker as a TYPED component (RFC-0043): the node is
# `NROS_C_COMPONENT(talker_t, talker_configure)` (src/Talker.c, a raw /chatter
# publisher + 1 Hz timer); `nano_ros_add_node(... TYPED ... DEPLOY native)`
# generates the C++ entry carrier (native_entry_main_c_typed.cpp.in) + the
# executable + links it on the real executor — no hand-written `nros_app_main`,
# no `nros_support_init`/`node_init`/`executor_init`/spin boilerplate (P1/P7).
#
find_package(nano_ros REQUIRED)

nano_ros_add_node(pkg_c_talker CLASS pkg_c_talker::Talker TYPED src/Talker.c DEPLOY native)

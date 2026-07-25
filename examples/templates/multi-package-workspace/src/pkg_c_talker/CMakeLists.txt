cmake_minimum_required(VERSION 3.22)
# C CXX: the node logic is a C typed component (src/Talker.c), but the generated
# native typed-entry carrier is C++ (drives NativeBoard::run_components).
project(pkg_c_talker LANGUAGES C CXX)

# Phase 140 / 244.C4 — multi-package workspace demo, add_subdirectory shape.
#
# C talker as a TYPED component (RFC-0043): the node is
# `NROS_C_COMPONENT(talker_t, talker_configure)` (src/Talker.c, a raw /chatter
# publisher + 1 Hz timer). RFC-0057 (phase-305) split shape: library owns
# sources, register owns identity — the generated C++ entry carrier + the
# executable come from the register (DEPLOY native), same as before.
find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(pkg_c_talker_lib STATIC src/Talker.c)

nros_components_register_node(pkg_c_talker_lib
    PLUGIN pkg_c_talker::Talker
    EXECUTABLE pkg_c_talker
    SHAPE configure
    TYPED
    DEPLOY native)

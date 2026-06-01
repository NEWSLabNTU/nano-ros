# Phase 212.H.7 fixture — talker component pkg.
#
# Each component pkg ships its own CMakeLists for the host-side
# compile-check (cargo nros plan inspects pkg shape; this stub keeps
# the pkg buildable in isolation if a user wires it into a non-PX4
# target later). PX4 SITL does NOT consume this file — the codegen
# emits a separate `px4_add_module()` skeleton into
# `$PX4_AUTOPILOT_DIR/src/modules/nros_talker/`.

cmake_minimum_required(VERSION 3.16)
project(talker_pkg CXX)
add_library(talker_pkg STATIC src/talker.cpp)
target_include_directories(talker_pkg PUBLIC src)

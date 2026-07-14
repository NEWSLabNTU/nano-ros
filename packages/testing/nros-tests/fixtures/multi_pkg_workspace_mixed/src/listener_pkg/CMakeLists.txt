# Phase 212.M.10 — C++ Component pkg (listener_pkg).
#
# Pre-212 used `component_nros.toml` + a hand `metadata/listener.json`
# sidecar; both are RETIRED per §212.L. The shape now is in-cmake:
# `nano_ros_node_register(NAME ... CLASS ... SOURCES ... DEPLOY)`
# writes a JSON fragment to the build dir under
# `nros-metadata.json` (consumed by `nros codegen-system`).
#
# (`nano_ros_deploy` was retired post-287 — the deploy/rmw tuple lives in
# package.xml.) The included module (NanoRosNodeRegister) is pulled in by the
# top-level CMakeLists so `@NANO_ROS_ROOT@` has already been substituted by
# the integration test's stage step.

# §212.L.4: pkg dir name IS pkg name; CLASS must start with `${PROJECT_NAME}::`.
# Sub-`project()` rescopes the L.4 check to the pkg dir name.
project(listener_pkg VERSION 0.1.0 LANGUAGES CXX)

nano_ros_node_register(
    NAME    listener
    CLASS   listener_pkg::Listener
    SOURCES src/listener.cpp
    DEPLOY  native)


# The consuming test (`cmake_mixed_corrosion_bridge_builds`) only
# asserts this executable builds. Corrosion bridges the Rust
# `talker_pkg` staticlib into the superbuild, but no linkage is
# required here — the link-correctness is what the test verifies.
add_executable(listener src/listener.cpp)

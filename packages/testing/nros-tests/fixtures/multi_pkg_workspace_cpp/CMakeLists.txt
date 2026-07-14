# Phase 212.M.10 fixture — pure C/C++ workspace under the §212.L
# Component pkg + Entry pkg taxonomy.
#
# Pre-212 sidecar TOML / `metadata/*.json` / `nros.toml` have all been
# retired (§212.L Non-Goals). Component metadata is declared at
# configure time via `nano_ros_node_register(...)` and the executable
# via `nano_ros_entry(...)`. Both fns ship from `cmake/` and emit
# `${CMAKE_BINARY_DIR}/nros-metadata.json` — no committed sidecar.
#
# `@NANO_ROS_ROOT@` is rewritten by the staging harness before configure
# (see `tests/phase212_d_workspace_metadata.rs::stage_fixture`).
cmake_minimum_required(VERSION 3.22)
project(multi_pkg_workspace_cpp C CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include("@NANO_ROS_ROOT@/cmake/NanoRosNodeRegister.cmake")

# Component pkgs — each its own `project()` so the L.4
# `CLASS startswith ${PROJECT_NAME}::` rule resolves correctly.
add_subdirectory(src/talker_pkg)
add_subdirectory(src/listener_pkg)

# Entry pkg — replaces the retired Bringup pkg per §212.L.3. Folds the
# old launch.xml / system.toml deploy declaration into the cmake fn.
add_subdirectory(src/demo_entry)

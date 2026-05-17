cmake_minimum_required(VERSION 3.16)
project(nros_rmw_cffi VERSION 0.1.0 LANGUAGES C)

# nros-rmw-cffi is primarily a Rust crate; the C ABI public headers
# (rmw_vtable.h, rmw_ret.h, rmw_entity.h, rmw_event.h) live under
# `include/nros/` and are reachable via the in-tree include path
# wired by the root `CMakeLists.txt` (`nros_c-static`'s INTERFACE).
#
# Phase 140 — install rules deleted. Out-of-tree backends consume
# nano-ros via `add_subdirectory(<repo-root>)`; the umbrella
# `NanoRos::NanoRos` target exposes these headers transitively.
#
# No library is built here — the Rust side produces `nros_rmw_cffi.a`
# via Cargo as part of the nros-cpp / nros-c builds.

# An INTERFACE library so sibling add_subdirectory'd consumers (e.g.
# packages/dds/nros-rmw-cyclonedds) can `target_link_libraries(...
# nros_rmw_cffi_headers)` without re-replicating the include path.
add_library(nros_rmw_cffi_headers INTERFACE)
target_include_directories(nros_rmw_cffi_headers INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/include")

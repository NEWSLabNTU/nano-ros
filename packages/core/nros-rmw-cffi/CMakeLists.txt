cmake_minimum_required(VERSION 3.16)
project(nros_rmw_cffi VERSION 0.1.0 LANGUAGES C)

# nros-rmw-cffi is primarily a Rust crate; this CMakeLists exists only
# to install the C ABI public headers (rmw_vtable.h, rmw_ret.h,
# rmw_entity.h, rmw_event.h) into the shared `build/install/` prefix
# alongside the rest of the NanoRos package, so out-of-tree backends
# (Phase 117 nros-rmw-cyclonedds and any future C++ backend) can link
# against `nros_rmw_cffi_register` and the vtable types via
# `find_package(NanoRos)`.
#
# No library is built here — the Rust side produces `nros_rmw_cffi.a`
# via Cargo as part of the nros-cpp / nros-c builds.

include(GNUInstallDirs)

install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
    FILES_MATCHING PATTERN "rmw_*.h"
)

# Phase 212.M.7 — ESP-IDF listener example.
#
# Self-contained `idf.py`-shaped project consuming the `nano-ros`
# component via `integrations/nano-ros/`. Build with:
#
#     source esp-idf-workspace/env.sh        # IDF_PATH + tools venv
#     idf.py -B build set-target esp32c3
#     idf.py -B build build
#
# See the sibling `talker/CMakeLists.txt` for the wiring rationale —
# the listener project shape is identical, only the Cargo crate name
# + `project(...)` identifier differ.

cmake_minimum_required(VERSION 3.16)

# Tests / packagers may override the nano-ros root via env. The
# default points at the in-tree example's natural depth
# (`<repo>/examples/esp32/rust/listener/` ↑ 4 = `<repo>/`).
if(DEFINED ENV{NANO_ROS_ROOT})
    set(_nros_root "$ENV{NANO_ROS_ROOT}")
else()
    get_filename_component(_nros_root
        "${CMAKE_CURRENT_LIST_DIR}/../../../.." ABSOLUTE)
endif()

set(EXTRA_COMPONENT_DIRS "${_nros_root}/integrations/nano-ros")

set(NROS_WORKSPACE_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE PATH "")

# Phase 212.M.7 — the integration shim references `${BUILD_DIR}/
# nros-system` in `INCLUDE_DIRS` unconditionally (only `file(
# MAKE_DIRECTORY)` it when `nros codegen-system` runs). Pre-create
# the dir here so configure succeeds even when no bringup pkg is
# wired (the example self-bringups via its own metadata).
file(MAKE_DIRECTORY "${CMAKE_BINARY_DIR}/nros-system")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(esp32_bsp_listener)

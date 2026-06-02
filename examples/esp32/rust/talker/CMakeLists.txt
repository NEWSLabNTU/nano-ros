# Phase 212.M.7 — ESP-IDF talker example.
#
# Self-contained `idf.py`-shaped project consuming the `nano-ros`
# component via `integrations/nano-ros/`. Build with:
#
#     source esp-idf-workspace/env.sh        # IDF_PATH + tools venv
#     idf.py -B build set-target esp32c3
#     idf.py -B build build
#
# The nano-ros component pulls the umbrella `NanoRos::NanoRos`
# (`nros-rmw-xrce-cffi-staticlib` + `nros-platform-freertos` shim);
# this project's `main/` component links it plus the example's own
# Rust staticlib (`esp32_bsp_talker`) exposing `rust_app_main()`.
#
# Phase 212.M.7 — the hardcoded `../../../../../integrations/nano-ros`
# relative path is intentional: the example is a copy-out template,
# so users who clone an installed nano-ros under `third_party/` need
# to repoint this once (the path is documented in the example README
# / `package.xml` description). Avoids the `@NANO_ROS_ROOT@`
# placeholder rewrite the H.5 fixture uses (a maintainer-side test
# concern; not appropriate for a user-facing template).

cmake_minimum_required(VERSION 3.16)

# Tests / packagers may override the nano-ros root via env. The
# default points at the in-tree example's natural depth
# (`<repo>/examples/esp32/rust/talker/` ↑ 4 = `<repo>/`).
if(DEFINED ENV{NANO_ROS_ROOT})
    set(_nros_root "$ENV{NANO_ROS_ROOT}")
else()
    get_filename_component(_nros_root
        "${CMAKE_CURRENT_LIST_DIR}/../../../.." ABSOLUTE)
endif()

set(EXTRA_COMPONENT_DIRS "${_nros_root}/integrations/nano-ros")

# Tell the nano-ros shim where the workspace + (future) bringup live.
# The example itself plays the bringup role until a multi-pkg sibling
# tree replaces it; the shim's `nros codegen-system` step fails soft
# when no `system.toml` is found, so this is a hint not a hard
# requirement.
set(NROS_WORKSPACE_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE PATH "")

# Phase 212.M.7 — the integration shim references `${BUILD_DIR}/
# nros-system` in `INCLUDE_DIRS` unconditionally (only `file(
# MAKE_DIRECTORY)` it when `nros codegen-system` runs). Pre-create
# the dir here so configure succeeds even when no bringup pkg is
# wired (the example self-bringups via its own metadata).
file(MAKE_DIRECTORY "${CMAKE_BINARY_DIR}/nros-system")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(esp32_bsp_talker)

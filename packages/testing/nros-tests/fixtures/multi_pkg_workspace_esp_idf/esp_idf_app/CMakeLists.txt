# Phase 212.H.5 fixture — minimal ESP-IDF project that consumes the
# Phase 139.2 / 212.H.5 nano-ros component.
#
# Mirrors `tests/esp-idf-smoke/CMakeLists.txt`. Pulls in the
# `integrations/nano-ros/` shim via EXTRA_COMPONENT_DIRS. NROS_WORKSPACE_DIR
# is bubbled to the shim so its configure-time `nros codegen-system`
# step can locate `demo_bringup/system.toml`.

cmake_minimum_required(VERSION 3.16)

# @NANO_ROS_ROOT@ is rewritten by the integration test before invoking
# `idf.py`. Same pattern as the Phase 212.D fixtures.
set(NANO_ROS_ROOT "@NANO_ROS_ROOT@")

set(EXTRA_COMPONENT_DIRS
    "${NANO_ROS_ROOT}/integrations/nano-ros")

# Tell the nano-ros shim where the workspace + bringup live.
set(NROS_WORKSPACE_DIR "${CMAKE_CURRENT_LIST_DIR}/.." CACHE PATH "")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(multi_pkg_workspace_esp_idf)

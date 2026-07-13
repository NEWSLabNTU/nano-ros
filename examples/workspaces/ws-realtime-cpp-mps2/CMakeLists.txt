cmake_minimum_required(VERSION 3.22)

# Phase 274.W3 (ws-realtime-cpp-mps2) — a cross-compiled embedded board needs its
# CMAKE_TOOLCHAIN_FILE set BEFORE project() (the first compiler probe). This
# workspace builds ONE board (mps2-an385-freertos) per configure; map the board
# key to its toolchain here. Mirrors examples/workspaces/cpp/CMakeLists.txt.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(ws_realtime_cpp_mps2 LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# Phase 274.W3 (RFC-0015 Model 1 embedded) — the FreeRTOS/mps2-an385 projection of
# ws-realtime-cpp: a control node + an aux node + a telemetry node whose bringup
# `system.toml [tiers.*]` + per-node callback groups resolve to a multi-tier entry
# via the entry codegen. The multi-tier FreeRTOS gate emits per-tier setup fns + a
# NativeTierSpec array + nros_app_main calling FreertosBoard::run_tiers. The node
# pkgs are listed BEFORE the entry so the entry codegen sees their
# `nano_ros_node_register` metadata.
#
# Phase 144 (#144) — the aux node adds a 3rd (mid) tier so the tier chain has two
# hops (boot high → mid → low): the mid tier is spawned BY a spawned tier, the
# exact tier↔tier declare race the chained-spawn fix serializes.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM freertos)
endif()

set(_ws_subdirs
    src/ctrl_pkg
    src/aux_pkg
    src/telem_pkg
    src/freertos_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})

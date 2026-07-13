cmake_minimum_required(VERSION 3.22)

# phase-281 W2 (ws-realtime-c-mps2) — a cross-compiled embedded board needs its
# CMAKE_TOOLCHAIN_FILE set BEFORE project() (the first compiler probe). This
# workspace builds ONE board (mps2-an385-freertos) per configure; map the board
# key to its toolchain here. Mirrors examples/workspaces/ws-realtime-cpp-mps2.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(ws_realtime_c_mps2 LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-281 W2 (RFC-0015 Model 1 embedded) — the FreeRTOS/mps2-an385 projection of
# ws-realtime-c: the C sibling of ws-realtime-cpp-mps2. A C control node + a C
# telemetry node whose bringup `system.toml [tiers.*]` + per-node callback groups
# resolve to a multi-tier entry via the entry codegen. The multi-tier FreeRTOS gate
# (board_is_freertos_embedded) emits per-tier setup fns + a NativeTierSpec array + an
# nros_app_main calling FreertosBoard::run_tiers — which delegates to the shared C
# `nros_board_freertos_run_tiers` glue. This proves that shared C run_tiers impl drives
# a C *node*, not only a C++ one (today only ws-realtime-cpp-mps2 exercises it).
#
# The node pkgs are listed BEFORE the entry so the entry codegen sees their
# `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM freertos)
endif()

set(_ws_subdirs
    src/ctrl_pkg
    src/telem_pkg
    src/freertos_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})

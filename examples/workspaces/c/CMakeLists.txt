cmake_minimum_required(VERSION 3.22)

# phase-263 C2b — a cross-compiled embedded board needs its CMAKE_TOOLCHAIN_FILE set
# BEFORE project() (the first compiler probe). The workspace builds ONE board per
# configure, so map the board key to its toolchain here; host-sim boards (threadx-linux)
# and posix need none. Keeps the embedded fixture rows simple (they pass only
# -DNANO_ROS_PLATFORM/-DNANO_ROS_BOARD; the workspace wires the rest), unlike the
# standalone qemu examples which rely on the recipe passing -DCMAKE_TOOLCHAIN_FILE.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    # phase-372 W2 — NXP S32Z270 RTU (Cortex-R52) cross cell.
    elseif(NANO_ROS_BOARD STREQUAL "s32z270-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcr52.cmake")
    endif()
endif()

project(pure_c_workspace LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-263 C2 — the workspace builds as ONE platform per configure (like the standalone
# embedded examples + the Rust per-fixture-row model). Default posix (native); an
# embedded fixture row passes `-DNANO_ROS_PLATFORM=<rtos> -DNANO_ROS_BOARD=<board>`. Only
# the entries matching the active platform are added, so a posix configure never processes
# an embedded entry's CMakeLists and vice versa. The node pkgs build for whichever platform
# is active.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/talker_pkg
    src/listener_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs
        # phase-263 A1 (services, C) — the AddTwoInts server/client Node pkgs + their two
        # single-node Entries (cross-process round-trip, issue 0096). Node pkgs BEFORE entries
        # so the entry codegen sees their nano_ros_node_register metadata. Native-only.
        src/service_server_pkg
        src/service_client_pkg
        # phase-263 A4 (actions, C) — the Fibonacci server/client Node pkgs + their two
        # single-node Entries (cross-process round-trip, issue 0096). Node pkgs BEFORE entries.
        src/action_server_pkg
        src/action_client_pkg
        src/native_entry
        src/native_entry_robot1
        src/native_entry_robot2
        src/native_service_server_entry
        src/native_service_client_entry
        src/native_action_server_entry
        src/native_action_client_entry)
elseif(NANO_ROS_BOARD STREQUAL "threadx-linux")
    list(APPEND _ws_subdirs src/threadx_entry)
elseif(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
# phase-372 W2 — the S32Z270 cross cell reuses the same embedded FreeRTOS entry.
elseif(NANO_ROS_BOARD STREQUAL "s32z270-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
# phase-370 W3 — the same FreeRTOS platform on the host: no toolchain file is
# mapped above, because the POSIX port compiles with the host compiler. That
# absence is the whole difference at this level.
elseif(NANO_ROS_BOARD STREQUAL "freertos-posix")
    list(APPEND _ws_subdirs src/freertos_posix_entry)
elseif(NANO_ROS_BOARD STREQUAL "nuttx-qemu-arm")
    list(APPEND _ws_subdirs src/nuttx_entry)
    # phase-263 C2b — the NuttX kernel-link custom command (cargo nros-nuttx-ffi) bakes the
    # NUTTX_DIR / NUTTX_APPS_DIR CMAKE vars at configure time. In a standalone example the
    # board overlay + carrier share one scope so the vars are visible; in this workspace the
    # entry pkg is a SIBLING subdir of `nano_ros`, so a directory-scope var set by the board
    # overlay never reaches it. Promote the env to CACHE (global) so every subdir sees it.
    if(DEFINED ENV{NUTTX_DIR} AND NOT NUTTX_DIR)
        set(NUTTX_DIR "$ENV{NUTTX_DIR}" CACHE PATH "NuttX kernel tree")
    endif()
    if(DEFINED ENV{NUTTX_APPS_DIR} AND NOT NUTTX_APPS_DIR)
        set(NUTTX_APPS_DIR "$ENV{NUTTX_APPS_DIR}" CACHE PATH "NuttX apps tree")
    endif()
endif()

# phase-348 W4 — ORDER_FROM_DEPENDS derives the add_subdirectory() order from
# each package's `<depend>` tags. The list above still chooses the SET (that is
# platform filtering, a selection no dependency can express) but no longer has
# to be written in a valid order: the entries already declare
# `<exec_depend>talker_pkg</exec_depend>` and friends, which is the same
# constraint the old "Node pkgs BEFORE entries" comments asked authors to keep
# by hand.
# phase-370 W3 — forward `-DNROS_RMW` instead of hardwiring the backend.
#
# `BACKEND zenoh` is authoritative over the cache variable by design (a
# workspace root is the one place that declares its system), so a row asking for
# another RMW LOST. phase-368 made that loud rather than silent, and the warning
# it added has been firing on rows that already exist: `workspace-c-native-cyclonedds`
# and `workspace-cpp-native-cyclonedds` pass `-DNROS_RMW=cyclonedds` — the `rmw`
# field expands to exactly that (`fixtures-manifest.py`) — and linked zenoh.
#
# The forwarding shape below is the one phase-368's own warning text prescribes
# and the scaffolded template ships. zenoh stays the DEFAULT, so every row that
# does not ask for something else is unchanged.
if(NOT DEFINED NROS_RMW)
    set(NROS_RMW zenoh)
endif()

nano_ros_workspace(
    BACKEND  ${NROS_RMW}
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    ORDER_FROM_DEPENDS
    SUBDIRS  ${_ws_subdirs})

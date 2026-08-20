cmake_minimum_required(VERSION 3.22)

# phase-263 C2c — a cross-compiled embedded board needs its CMAKE_TOOLCHAIN_FILE set
# BEFORE project() (the first compiler probe). The workspace builds ONE board per
# configure; map the board key to its toolchain. Host-sim (threadx-linux) + posix need
# none. Mirrors examples/workspaces/{c,cpp}/CMakeLists.txt.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(c_and_cpp_mixed_workspace LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-263 C2c — ONE platform per configure (the Rust+C+C++ mix is combined by the
# `nros_ws_runtime` umbrella). Default posix; an embedded fixture row passes
# `-DNANO_ROS_PLATFORM=<rtos> -DNANO_ROS_BOARD=<board>`.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_talker_pkg
    src/cpp_listener_pkg
    src/rust_heartbeat_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs
        # phase-263 A1 (services, MIXED) — cross-LANGUAGE AddTwoInts: C server + C++
        # client pkgs + their two single-node entries (cross-process, issue 0096).
        # Pkgs before entries. The C++ pkg in the MIXED multi-pkg generation is the
        # #203 regression site (the action_msgs cpp FFI over-generation used to fail
        # here; resolved — the pair is now genuinely cross-language).
        src/c_add_server_pkg
        src/cpp_add_client_pkg
        # phase-263 A4 (actions, MIXED) — Fibonacci C server + C client pkgs + their two
        # single-node entries (cross-process, issue 0096). Pkgs before entries.
        src/c_fib_server_pkg
        src/c_fib_client_pkg
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
endif()

# phase-348 W4 — the add_subdirectory() order comes from each package's
# `<depend>` tags. The list still chooses the SET (platform filtering is a
# selection no dependency can express); it no longer has to be written in a
# valid order.
# phase-370 W3 — forward `-DNROS_RMW` instead of hardwiring the backend.
#
# `BACKEND zenoh` is authoritative over the cache variable by design (a
# workspace root is the one place that declares its system), so a row asking for
# another RMW LOST. phase-368 made that loud rather than silent, and the warning
# it added has been firing on rows that already exist: the `native-cyclonedds`
# workspace rows pass `-DNROS_RMW=cyclonedds` — the `rmw` field expands to
# exactly that (`fixtures-manifest.py`) — and linked zenoh.
#
# The forwarding shape below is the one phase-368's own warning text prescribes
# and the scaffolded template ships. zenoh stays the DEFAULT, so every row that
# does not ask for something else is unchanged.
if(NOT DEFINED NROS_RMW)
    set(NROS_RMW zenoh)
endif()

nano_ros_workspace(
    ORDER_FROM_DEPENDS
    BACKEND  ${NROS_RMW}
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})

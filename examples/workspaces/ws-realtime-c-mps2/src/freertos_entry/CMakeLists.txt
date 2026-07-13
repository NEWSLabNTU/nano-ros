cmake_minimum_required(VERSION 3.22)
project(freertos_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-281 W2 — embedded C multi-tier entry on FreeRTOS/mps2-an385 (QEMU).
#
# The codegen emitter (`nros codegen entry --lang c --board mps2-an385-freertos`)
# routes embedded-C through the C++ emitter (a C entry TU can't call the C++
# `FreertosBoard::run_tiers` runner directly, so the generated TU is `.cpp` and
# instantiates each C node via its `NROS_C_COMPONENT` `extern "C"` seam). It
# reads the 2-tier system.toml (ctrl→high/freertos-pri=5, telem→low/freertos-pri=2)
# and — with the FreeRTOS embedded gate (`board_is_freertos_embedded`) — emits:
#   * per-tier setup functions (__nros_entry_setup_tier_0, _1)
#   * NativeTierSpec array __nros_tiers[2]
#   * extern "C" int nros_app_main(...) calling FreertosBoard::run_tiers(...)
#   * NROS_APP_MAIN_REGISTER_VOID()
#
# FreertosBoard::run_tiers delegates to the SHARED C impl
# nros_board_freertos_run_tiers (nros-board-freertos glue), which opens ONE
# session and spawns one RTOS task per non-boot tier (RFC-0015 Model 1 embedded).
# This is the same C run_tiers glue the C++ freertos e2e exercises — here it
# drives C *nodes*, closing the C × freertos matrix cell.
nano_ros_add_executable(freertos_entry
    BOARD   mps2-an385-freertos
    LAUNCH  "demo_bringup:system.launch.xml"
    LANG    c
    TYPED
    DEPLOY  mps2-an385-freertos)

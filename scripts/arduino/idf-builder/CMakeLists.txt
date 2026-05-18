# Phase 23.2 — IDF project used by `scripts/arduino/build-libnanoros.sh`
# to cross-compile every nano-ros static archive needed inside the
# Arduino library (`libnros_c.a`, `libzpico.a`, `libnros_platform_esp_idf.a`,
# etc.). It is not user-facing — Arduino sketches consume the bundled
# `arduino/nros/src/<arch>/libnanoros.a` directly.
#
# Layout:
#   main/                          — minimal app stub (IDF requires a `main`)
#   ../../../integrations/esp-idf  — pulled as a component via
#                                    EXTRA_COMPONENT_DIRS
#   ../../../packages/core/nros-platform-esp-idf  — child component

cmake_minimum_required(VERSION 3.16)

set(EXTRA_COMPONENT_DIRS
    "${CMAKE_CURRENT_LIST_DIR}/../../../integrations/esp-idf"
    "${CMAKE_CURRENT_LIST_DIR}/../../../packages/core/nros-platform-esp-idf"
    "${CMAKE_CURRENT_LIST_DIR}/../../../packages/codegen/interfaces")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(nano_ros_arduino_lib_builder)

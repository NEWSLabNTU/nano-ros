# Minimal ESP-IDF project that exercises `nros-platform-esp-idf`.
#
# Run via `just esp_idf build-c-port` (which sources
# esp-idf-workspace/env.sh + invokes idf.py). The project pulls the
# nros-platform-esp-idf component via EXTRA_COMPONENT_DIRS.

cmake_minimum_required(VERSION 3.16)

set(EXTRA_COMPONENT_DIRS
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-esp-idf")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(nros_esp_idf_smoke)

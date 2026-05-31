# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (C++)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_action_server_cpp LANGUAGES CXX C)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)

if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
endif()

target_sources(app PRIVATE src/main.cpp)

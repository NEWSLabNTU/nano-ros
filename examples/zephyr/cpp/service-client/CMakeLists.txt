# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Service Client Example (C++)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_service_client_cpp LANGUAGES CXX)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

nros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    LANGUAGE CPP
)


# Phase 11W.12 — Cyclone DDS needs C `dds_topic_descriptor_t`s for each
# wire type. A service maps to a request + response topic, so generate
# both descriptors from example_interfaces/srv/AddTwoInts.srv (the
# converter emits the _Request_ / _Response_ structs) and compile them
# into the app.
if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   example_interfaces
        PKG_DIR    "$ENV{NROS_EXAMPLE_INTERFACES_DIR}"
        INTERFACES srv/AddTwoInts.srv)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

target_sources(app PRIVATE src/main.cpp)

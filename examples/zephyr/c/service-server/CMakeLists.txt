# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Service Server Example (C, Zenoh)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_zenoh_service_server_c)

nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv")

if(CONFIG_NROS_RMW_CYCLONEDDS)
    set(_nros_repo ${CMAKE_CURRENT_LIST_DIR}/../../../..)
    set(IDLC_EXECUTABLE "${_nros_repo}/build/install/bin/idlc"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${_nros_repo}/scripts/cyclonedds")
    include("${_nros_repo}/packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   example_interfaces
        PKG_DIR    /opt/ros/humble/share/example_interfaces
        INTERFACES srv/AddTwoInts.srv)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

target_sources(app PRIVATE src/main.c)

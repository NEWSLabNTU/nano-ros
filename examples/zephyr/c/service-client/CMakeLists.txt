# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Service Client Example (C) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.c`, the node is a stateful C
# component (`NROS_C_COMPONENT(add_client_t, client_configure)`); the Zephyr typed
# carrier (the shared entry pack, a C++ entry that drives the C
# component's `__nros_c_component_<pkg>_{create,configure}` seam) generates the
# entry + composes it into the Zephyr `app`.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_zenoh_service_client_c)

# package.xml SSoT (issue #11): resolve + generate the transitive interface
# closure declared in package.xml (<depend>example_interfaces</depend>) into `app`.

# Cyclone DDS topic descriptors for this image's interface packages are generated
# by the shared interface pipeline (`zephyr/cmake/nros_generate_interfaces.cmake`)
# when `find_package(<pkg>)` runs below. A hand-written descriptor block here
# used to generate the same descriptors a second time; with the global
# `--allow-multiple-definition` gone (issue 1636) that was a duplicate-symbol
# link error, so it is gone too (issue 1673).

# The Zephyr typed carrier branch (LANGUAGE C) configure_files
# the shared entry pack (`nros codegen entry-node`) + compiles src/AddTwoIntsClient.c with
# -DNROS_PKG_NAME=<pkg sym> so its NROS_C_COMPONENT factory/configure seam links
# against the generated entry, all into the global `app`. No second executable.
find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

nano_ros_auto_add_library(add_two_ints_client_lib STATIC src/AddTwoIntsClient.c)

nros_components_register_node(add_two_ints_client_lib
    PLUGIN nros_zephyr_zenoh_service_client_c::AddTwoIntsClient
    EXECUTABLE add_two_ints_client
    SHAPE configure
    TYPED)

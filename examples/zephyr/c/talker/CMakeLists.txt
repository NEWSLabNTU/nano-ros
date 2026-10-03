# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker (C) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.c`, the node is a stateful C
# component (`NROS_C_COMPONENT(talker_t, talker_configure)`); the Zephyr typed
# carrier (the shared entry pack) generates the entry + composes it
# into `app`. The generated typed C bindings (std_msgs) provide the message
# struct + serializer (phase-277 W4; was hand-rolled CDR).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_talker_c)

# package.xml SSoT: generate the typed C bindings for the interface closure
# (std_msgs) into `app` — same call the zephyr C service/action examples use.

# Cyclone DDS topic descriptors for this image's interface packages are generated
# by the shared interface pipeline (`zephyr/cmake/nros_generate_interfaces.cmake`)
# when `find_package(<pkg>)` runs below. A hand-written descriptor block here
# used to generate the same descriptors a second time; with the global
# `--allow-multiple-definition` gone (issue 1636) that was a duplicate-symbol
# link error, so it is gone too (issue 1673).

# The Zephyr typed carrier branch (LANGUAGE C) configure_files
# the shared entry pack (`nros codegen entry-node`) + compiles src/Talker.c with
# -DNROS_PKG_NAME=<pkg sym> so its NROS_C_COMPONENT seam links against the
# generated entry, all into the global `app`. No second executable.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN nros_zephyr_talker_c::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED)

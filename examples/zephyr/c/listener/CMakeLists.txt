# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Listener Example (C) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.c`, the node is a stateful C
# component (`NROS_C_COMPONENT(listener_t, listener_configure)`); the Zephyr typed
# carrier (the shared entry pack, a C++ entry that drives the C
# component's `__nros_c_component_<pkg>_{create,configure}` seam) generates the
# entry + composes it into the Zephyr `app`. The generated typed C bindings
# (std_msgs) provide the message struct + deserializer (phase-277 W4; was
# hand-rolled CDR).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_listener_c)

# nros module (CONFIG_NROS=y, CONFIG_NROS_C_API=y) provides zenoh shim,
# platform support, and the nros-c library automatically.

# package.xml SSoT: generate the typed C bindings for the interface closure
# (std_msgs) into `app` — same call the zephyr C service/action examples use.

# Cyclone DDS topic descriptors for this image's interface packages are generated
# by the shared interface pipeline (`zephyr/cmake/nros_generate_interfaces.cmake`)
# when `find_package(<pkg>)` runs below. A hand-written descriptor block here
# used to generate the same descriptors a second time; with the global
# `--allow-multiple-definition` gone (issue 1636) that was a duplicate-symbol
# link error, so it is gone too (issue 1673).

# The Zephyr typed carrier branch (LANGUAGE C) configure_files
# the shared entry pack (`nros codegen entry-node`) + compiles src/Listener.c with
# -DNROS_PKG_NAME=<pkg sym> so its NROS_C_COMPONENT factory/configure seam links
# against the generated entry, all into the global `app`. No second executable.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_node(listener CLASS nros_zephyr_listener_c::Listener TYPED src/Listener.c)

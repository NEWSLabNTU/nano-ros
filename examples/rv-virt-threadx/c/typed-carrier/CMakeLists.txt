cmake_minimum_required(VERSION 3.22)
# C CXX: the node logic is a C typed component (src/Talker.c); the ThreadX
# typed-entry carrier `nros_components_register_node` generates is C++.
project(c_typed_carrier LANGUAGES C CXX)

# issue 1795 — the RTOS TYPED CARRIER road. Every other rv-virt-threadx example
# links through `nano_ros_add_executable()` (the entry road); this one
# registers a typed component with `DEPLOY threadx`, so the image comes from
# `nano_ros_node_register`'s ThreadX carrier arm, which synthesises a C++ entry
# holding the component's node as a STATIC object. That road failed to link
# twice (issues 1742, 1753) with nothing building it; this leaf is its fixture.
find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(c_typed_carrier_lib STATIC src/Talker.c)

nros_components_register_node(c_typed_carrier_lib
    PLUGIN riscv64_threadx_c_typed_carrier::Talker
    EXECUTABLE c_typed_carrier
    SHAPE configure
    TYPED
    DEPLOY threadx)

cmake_minimum_required(VERSION 3.22)
# C CXX: the node logic is a C typed component (src/Talker.c); the NuttX
# typed-entry carrier `nros_components_register_node` generates is C++.
project(c_typed_carrier LANGUAGES C CXX)

# issue 1795 — the RTOS TYPED CARRIER road. Every other qemu-armv7a-nuttx example
# links through `nano_ros_add_executable()` (the entry road); this one
# registers a typed component with `DEPLOY nuttx`, so the image comes from
# `nano_ros_node_register`'s NuttX carrier arm, which synthesises a C++ entry
# holding the component's node as a STATIC object. On a lane build
# (`NROS_PLATFORM=nuttx_armv7a`) that arm never fired until issue 1795; this
# leaf is its fixture.
find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(c_typed_carrier_lib STATIC src/Talker.c)

nros_components_register_node(c_typed_carrier_lib
    PLUGIN nuttx_c_typed_carrier::Talker
    EXECUTABLE c_typed_carrier
    SHAPE configure
    TYPED
    DEPLOY nuttx)

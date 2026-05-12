cmake_minimum_required(VERSION 3.16)
project(c_custom_transport_loopback
    VERSION 0.1.0
    LANGUAGES C
)

# Phase 115.F — runtime-pluggable custom transport demonstration in
# pure C. The example installs a ring-buffer-backed
# `nros_transport_ops_t` via `nros_set_custom_transport`, then
# opens an nros session over the custom byte pipe and publishes a
# few std_msgs/String frames. The test passes if (a) install
# returns NROS_RET_OK, (b) the transport's `open` + `write`
# callbacks fire during session bring-up, and (c) `read` returns
# the previously-written bytes (loopback).
#
# Demonstrates the contract documented in
# `packages/core/nros-rmw-cffi/include/nros/rmw_transport.h`:
# no concurrent read/write, no ISR, four fn pointers + opaque
# `user_data`. The same contract applies on bare-metal /
# RTOS / desktop builds — the example targets POSIX for
# portability but every line of transport-callback code below
# would compile unchanged on Cortex-M.

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG)

# Generate C bindings for std_msgs/String — the topic payload.
nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_custom_transport_loopback src/main.c)
target_link_libraries(c_custom_transport_loopback
    PRIVATE
        std_msgs__nano_ros_c
        NanoRos::NanoRos
)

# Pthreads for the wake condvar — the loopback's `read` callback
# blocks until the matching `write` fills the buffer. On
# bare-metal this would be a sem_take / WFE wake; POSIX uses
# pthread_cond_wait.
find_package(Threads REQUIRED)
target_link_libraries(c_custom_transport_loopback PRIVATE Threads::Threads)

install(TARGETS c_custom_transport_loopback RUNTIME DESTINATION bin)

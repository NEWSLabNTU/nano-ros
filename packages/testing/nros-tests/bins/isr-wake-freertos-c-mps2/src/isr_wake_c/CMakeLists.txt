cmake_minimum_required(VERSION 3.22)
# A C component that registers a device event source and signals its
# completion from the CMSDK TIMER1 interrupt (phase 579 W1b). No `main()`:
# the image's generated entry owns the executor and spins it.
project(isr_wake_c LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(isr_wake_lib STATIC src/IsrWake.c)

nros_components_register_node(isr_wake_lib
    PLUGIN isr_wake_c::IsrWake
    EXECUTABLE isr_wake
    SHAPE configure
    TYPED)

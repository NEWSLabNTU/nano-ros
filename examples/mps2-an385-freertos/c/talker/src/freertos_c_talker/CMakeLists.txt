cmake_minimum_required(VERSION 3.22)
# The ROS 2 tutorial talker as a typed C component (RFC-0043). No `main()`:
# the image's generated entry owns the support, node and executor lifecycle
# (RFC-0122 D10), and the class prefix is PROJECT_NAME (phase-212.L.4).
project(freertos_c_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN freertos_c_talker::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED)

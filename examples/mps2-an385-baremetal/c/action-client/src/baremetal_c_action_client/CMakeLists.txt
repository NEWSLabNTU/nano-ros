cmake_minimum_required(VERSION 3.22)
project(baremetal_c_action_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# A typed C component. It names no board: the bringup's image does, and the
# generated entry boots it.
nano_ros_add_node(fib_client CLASS baremetal_c_action_client::FibClient TYPED
    SOURCES src/FibClient.c)

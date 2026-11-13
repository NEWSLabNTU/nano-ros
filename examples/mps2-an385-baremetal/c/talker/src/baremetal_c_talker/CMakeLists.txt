cmake_minimum_required(VERSION 3.22)
project(baremetal_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# A typed C component. It names no board: the bringup's image does, and the
# generated entry boots it.
nano_ros_add_node(talker CLASS baremetal_c_talker::Talker TYPED
    SOURCES src/talker.c)

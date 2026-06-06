cmake_minimum_required(VERSION 3.22)
project(pure_c_workspace LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  src/c_talker_pkg
             src/c_listener_pkg
             src/native_entry)

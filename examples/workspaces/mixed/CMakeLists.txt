cmake_minimum_required(VERSION 3.22)
project(c_and_cpp_mixed_workspace LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  src/c_talker_pkg
             src/cpp_listener_pkg
             src/rust_heartbeat_pkg
             src/native_entry)

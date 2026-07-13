cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Raw-CDR typed component (RFC-0043): the raw `/reading` subscription carries the
# workspace-local type name as a string and hand-decodes the CDR payload (see
# src/ReadingListener.cpp). No `nros_find_interfaces` / interface archive link.
nano_ros_add_node(reading_listener CLASS reading_listener_pkg::ReadingListener
    src/ReadingListener.cpp)

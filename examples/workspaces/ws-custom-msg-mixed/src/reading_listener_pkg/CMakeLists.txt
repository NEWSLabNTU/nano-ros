cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043): the raw `/reading` subscription carries the
# workspace-local type name as a string, so no generated C bindings are needed —
# the CDR payload is hand-decoded (custom_msgs/Reading, see src/ReadingListener.c).
nano_ros_add_node(reading_listener CLASS reading_listener_pkg::ReadingListener
    TYPED
    src/ReadingListener.c)

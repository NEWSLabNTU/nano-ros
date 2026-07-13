cmake_minimum_required(VERSION 3.22)
project(c_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Typed component (RFC-0043): raw `/chatter` subscription carries the type name
# as a string, so no generated C bindings are needed.
nano_ros_add_node(listener CLASS c_listener_pkg::Listener TYPED src/Listener.c)

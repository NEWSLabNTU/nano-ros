cmake_minimum_required(VERSION 3.22)
project(c_param_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.

# Typed component (RFC-0043): publishes on /chatter via the generated
# std_msgs/Int32 serializer; reads publish_period_ms live via
# nros_cpp_get_param_integer.
nano_ros_auto_add_library(c_param_talker_lib STATIC src/Talker.c)

nros_components_register_node(c_param_talker_lib
    PLUGIN c_param_talker_pkg::Talker
    EXECUTABLE c_param_talker
    SHAPE configure
    TYPED)

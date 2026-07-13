cmake_minimum_required(VERSION 3.22)
project(qos_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Typed component (RFC-0043): a typed `Publisher<std_msgs::msg::Int32>` on /chatter.
# The differentiator vs the committed talker_pkg is the QoS: this declares a
# NON-DEFAULT profile via the `nros::QoS` builder
# (`.reliable().transient_local().keep_last(10)`) rather than the default profile
# (see src/QosTalker.cpp).
nano_ros_add_node(qos_talker CLASS qos_talker_pkg::QosTalker src/QosTalker.cpp)

# `nros_find_interfaces` declares the `std_msgs__nano_ros_cpp` INTERFACE lib
# carrying the generated headers' include dirs; guard with `if(TARGET)` (the
# Zephyr generator whole-archives the interface into `app` instead).
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(qos_talker_pkg_qos_talker_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()

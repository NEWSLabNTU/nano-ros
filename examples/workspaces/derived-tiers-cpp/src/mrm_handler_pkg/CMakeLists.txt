cmake_minimum_required(VERSION 3.22)
project(mrm_handler_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# phase-459 W0 - mrm_handler: one wall timer (100 ms, 10 Hz) publishing
# std_msgs/Int32 on /system/fail_safe/mrm_state. CALLBACK_GROUPS declares the one
# group the derivation keys on; the group is never created in code, so the
# whole node is placed on the derived tier (node-level placement). No
# system.toml row binds it and no [tiers.*] names a priority: the rate in
# demo_bringup/launch/system.contract.yaml is the only scheduling input.
nano_ros_auto_add_library(mrm_handler_lib STATIC src/MrmHandler.cpp)

nros_components_register_node(mrm_handler_lib
    HEADER mrm_handler_pkg/MrmHandler.hpp
    PLUGIN mrm_handler_pkg::MrmHandler
    EXECUTABLE mrm_handler
    SHAPE rclcpp
    CALLBACK_GROUPS main)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(mrm_handler_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()

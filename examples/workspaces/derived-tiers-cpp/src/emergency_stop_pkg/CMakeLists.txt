cmake_minimum_required(VERSION 3.22)
project(emergency_stop_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# phase-459 W0 - mrm_emergency_stop_operator: one wall timer (33 ms, 30 Hz) publishing
# std_msgs/Int32 on /system/emergency/control_cmd. CALLBACK_GROUPS declares the one
# group the derivation keys on; the group is never created in code, so the
# whole node is placed on the derived tier (node-level placement). No
# system.toml row binds it and no [tiers.*] names a priority: the rate in
# demo_bringup/launch/system.contract.yaml is the only scheduling input.
nano_ros_auto_add_library(emergency_stop_lib STATIC src/MrmEmergencyStopOperator.cpp)

nros_components_register_node(emergency_stop_lib
    HEADER emergency_stop_pkg/MrmEmergencyStopOperator.hpp
    PLUGIN emergency_stop_pkg::MrmEmergencyStopOperator
    EXECUTABLE mrm_emergency_stop_operator
    SHAPE rclcpp
    CALLBACK_GROUPS main)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(emergency_stop_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()

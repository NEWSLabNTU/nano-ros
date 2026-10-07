cmake_minimum_required(VERSION 3.22)
project(violation_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-474 T4 - handler: one wall timer (100 ms) publishing std_msgs/Int32
# on /t4/state, the path the contract bounds at 50 ms.
nano_ros_auto_add_library(violation_handler_lib STATIC src/Handler.cpp)

nros_components_register_node(violation_handler_lib
    HEADER violation_pkg/Handler.hpp
    PLUGIN violation_pkg::Handler
    EXECUTABLE handler
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(violation_handler_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()

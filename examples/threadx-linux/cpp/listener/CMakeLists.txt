cmake_minimum_required(VERSION 3.22)
project(threadx_linux_cpp_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")
find_package(std_msgs REQUIRED)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosComponentRegister.cmake")
nano_ros_component_register(
    NAME    listener
    CLASS   threadx_linux_cpp_listener::Listener
    SOURCES src/Listener.cpp
    DEPLOY  threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_cpp_listener src/main.c)
target_link_libraries(threadx_cpp_listener PRIVATE
    threadx_linux_cpp_listener_listener_component
    std_msgs::std_msgs)
nros_threadx_link_app(threadx_cpp_listener)
nano_ros_link_rmw(threadx_cpp_listener RMW "${NROS_RMW}")

cmake_minimum_required(VERSION 3.22)
# Phase 244 D6 — declarative Node pkg; class prefix must equal PROJECT_NAME (L.4).
project(threadx_c_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.8 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Phase 244 D6 — Component pkg shape (mirrors threadx-linux/cpp/listener).
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)
nano_ros_node_register(
    NAME     listener
    CLASS    threadx_c_listener::Listener
    LANGUAGE C
    SOURCES  src/Listener.c
    DEPLOY   threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_c_listener src/main.c)
target_link_libraries(threadx_c_listener PRIVATE
    threadx_c_listener_listener_component
    std_msgs__nano_ros_c)
nros_threadx_link_app(threadx_c_listener)
nano_ros_link_rmw(threadx_c_listener RMW "${NROS_RMW}")

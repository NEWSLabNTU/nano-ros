cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_cpp_action_client VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NANO_ROS_RMW      zenoh)
set(NANO_ROS_BOARD    riscv64-qemu)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

add_executable(riscv64_threadx_cpp_action_client src/main.cpp)
target_include_directories(riscv64_threadx_cpp_action_client PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(riscv64_threadx_cpp_action_client PRIVATE
    example_interfaces__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(riscv64_threadx_cpp_action_client)
nano_ros_link_rmw(riscv64_threadx_cpp_action_client RMW zenoh)

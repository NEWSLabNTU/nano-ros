cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_cpp_service_server LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nano_ros_generate_interfaces(example_interfaces LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_service_server src/main.cpp)
target_link_libraries(freertos_cpp_service_server PRIVATE
    example_interfaces__nano_ros_cpp NanoRos::NanoRosCpp freertos_platform)

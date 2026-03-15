cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_c_action_server LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nano_ros_generate_interfaces(example_interfaces "action/Fibonacci.action" LANGUAGE C SKIP_INSTALL)

add_executable(freertos_c_action_server src/main.c ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_c_action_server PRIVATE ${FREERTOS_STARTUP_INCLUDES})
target_link_libraries(freertos_c_action_server PRIVATE
    example_interfaces__nano_ros_c NanoRos::NanoRos freertos_platform)

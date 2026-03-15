cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_c_talker LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

add_executable(freertos_c_talker src/main.c ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_c_talker PRIVATE ${FREERTOS_STARTUP_INCLUDES})
target_link_libraries(freertos_c_talker PRIVATE
    std_msgs__nano_ros_c NanoRos::NanoRos freertos_platform)

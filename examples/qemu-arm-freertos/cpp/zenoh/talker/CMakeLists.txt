cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_cpp_talker LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nano_ros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nano_ros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_talker src/main.cpp ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_cpp_talker PRIVATE ${FREERTOS_STARTUP_INCLUDES})
target_link_libraries(freertos_cpp_talker PRIVATE
    std_msgs__nano_ros_cpp NanoRos::NanoRosCpp freertos_platform)

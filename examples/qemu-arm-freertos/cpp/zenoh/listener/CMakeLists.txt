cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_cpp_listener LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_listener src/main.cpp)
target_compile_definitions(freertos_cpp_listener PRIVATE
    APP_MAC="{0x02,0x00,0x00,0x00,0x00,0x01}"
    APP_IP="{192,0,3,11}"
)
target_link_libraries(freertos_cpp_listener PRIVATE
    std_msgs__nano_ros_cpp NanoRos::NanoRosCpp freertos_platform)

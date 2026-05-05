cmake_minimum_required(VERSION 3.22)
project(nuttx_c_talker LANGUAGES C CXX)

find_package(NanoRos CONFIG REQUIRED)
include(nuttx-support)

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

nuttx_build_example(nuttx_c_talker
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.c"
    INCLUDE_DIRS "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/std_msgs"
                 "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c"
    SOURCES      ${std_msgs_GENERATED_SOURCES}
)

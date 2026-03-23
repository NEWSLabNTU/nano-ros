cmake_minimum_required(VERSION 3.22)
project(nuttx_c_listener LANGUAGES C CXX)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/nuttx-support.cmake")

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")

nuttx_build_example(nuttx_c_listener
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.c"
    INCLUDE_DIRS "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/std_msgs"
                 "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c"
    COMPILE_DEFS "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
                 "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)

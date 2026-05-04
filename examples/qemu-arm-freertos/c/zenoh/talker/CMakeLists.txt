cmake_minimum_required(VERSION 3.22)
project(freertos_c_talker LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-support.cmake")

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

add_executable(freertos_c_talker src/main.c ${FREERTOS_STARTUP_SOURCE})
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_include_directories(freertos_c_talker PRIVATE
    ${FREERTOS_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
# Phase 112.D: APP_* macros remain for startup.c (which is per-example
# compiled and references APP_IP/APP_MAC for the lwIP netif). main.c uses
# the typed NROS_APP_CONFIG struct from the generated <nros/app_config.h>.
target_compile_definitions(freertos_c_talker PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_PRIORITY=${NROS_CONFIG_APP_PRIORITY}"
    "APP_STACK_BYTES=${NROS_CONFIG_APP_STACK_BYTES}"
    "APP_ZENOH_READ_PRIORITY=${NROS_CONFIG_ZENOH_READ_PRIORITY}"
    "APP_ZENOH_READ_STACK_BYTES=${NROS_CONFIG_ZENOH_READ_STACK_BYTES}"
    "APP_ZENOH_LEASE_PRIORITY=${NROS_CONFIG_ZENOH_LEASE_PRIORITY}"
    "APP_ZENOH_LEASE_STACK_BYTES=${NROS_CONFIG_ZENOH_LEASE_STACK_BYTES}"
    "APP_POLL_PRIORITY=${NROS_CONFIG_POLL_PRIORITY}"
    "APP_POLL_INTERVAL_MS=${NROS_CONFIG_POLL_INTERVAL_MS}"
)
target_link_libraries(freertos_c_talker PRIVATE
    std_msgs__nano_ros_c NanoRos::NanoRos freertos_platform)

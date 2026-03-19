cmake_minimum_required(VERSION 3.22)
project(freertos_c_talker LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-support.cmake")

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

add_executable(freertos_c_talker src/main.c ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_c_talker PRIVATE ${FREERTOS_STARTUP_INCLUDES})
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
target_compile_definitions(freertos_c_talker PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
    "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
target_link_libraries(freertos_c_talker PRIVATE
    std_msgs__nano_ros_c NanoRos::NanoRos freertos_platform)

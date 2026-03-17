cmake_minimum_required(VERSION 3.22)
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/arm-none-eabi-toolchain.cmake"
    CACHE FILEPATH "")
project(freertos_cpp_talker LANGUAGES C CXX ASM)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-platform.cmake")

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_talker src/main.cpp ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_cpp_talker PRIVATE ${FREERTOS_STARTUP_INCLUDES})
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
target_compile_definitions(freertos_cpp_talker PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
    "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
target_link_libraries(freertos_cpp_talker PRIVATE
    std_msgs__nano_ros_cpp NanoRos::NanoRosCpp freertos_platform)

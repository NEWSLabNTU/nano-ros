cmake_minimum_required(VERSION 3.22)
project(threadx_cpp_service_server LANGUAGES C CXX)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/threadx-support.cmake")

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

add_executable(threadx_cpp_service_server src/main.cpp ${THREADX_STARTUP_SOURCE})
target_include_directories(threadx_cpp_service_server PRIVATE ${THREADX_STARTUP_INCLUDES})
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
target_compile_definitions(threadx_cpp_service_server PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
    "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
target_link_libraries(threadx_cpp_service_server PRIVATE
    example_interfaces__nano_ros_cpp NanoRos::NanoRosCpp threadx_platform)

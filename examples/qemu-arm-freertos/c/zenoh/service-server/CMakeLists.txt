cmake_minimum_required(VERSION 3.22)
project(freertos_c_service_server LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include(freertos-support)

nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" LANGUAGE C SKIP_INSTALL)

add_executable(freertos_c_service_server src/main.c ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_c_service_server PRIVATE ${FREERTOS_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_link_libraries(freertos_c_service_server PRIVATE
    example_interfaces__nano_ros_c NanoRos::NanoRos freertos_platform)

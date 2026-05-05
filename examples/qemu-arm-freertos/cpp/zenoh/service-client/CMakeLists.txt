cmake_minimum_required(VERSION 3.22)
project(freertos_cpp_service_client LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include(freertos-support)

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

add_executable(freertos_cpp_service_client src/main.cpp ${FREERTOS_STARTUP_SOURCE})
target_include_directories(freertos_cpp_service_client PRIVATE ${FREERTOS_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_link_libraries(freertos_cpp_service_client PRIVATE
    example_interfaces__nano_ros_cpp NanoRos::NanoRosCpp freertos_platform)

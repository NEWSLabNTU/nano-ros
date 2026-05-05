cmake_minimum_required(VERSION 3.22)
project(threadx_c_listener LANGUAGES C CXX)

set(NANO_ROS_PLATFORM threadx_linux)
find_package(NanoRos CONFIG REQUIRED)
include(threadx-support)

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

add_executable(threadx_c_listener src/main.c ${THREADX_STARTUP_SOURCE} ${THREADX_APP_DEFINE_SOURCE})
target_include_directories(threadx_c_listener PRIVATE ${THREADX_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_compile_definitions(threadx_c_listener PRIVATE ${THREADX_GLUE_DEFINES})
target_link_libraries(threadx_c_listener PRIVATE
    std_msgs__nano_ros_c NanoRos::NanoRos threadx_platform)

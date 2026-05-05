cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_c_action_server LANGUAGES C CXX ASM)

set(NANO_ROS_PLATFORM threadx_riscv64)
find_package(NanoRos CONFIG REQUIRED)
include(threadx-riscv64-support)

nros_generate_interfaces(example_interfaces "action/Fibonacci.action" LANGUAGE C SKIP_INSTALL)

add_executable(riscv64_threadx_c_action_server src/main.c ${THREADX_STARTUP_SOURCE})
target_include_directories(riscv64_threadx_c_action_server PRIVATE ${THREADX_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_link_options(riscv64_threadx_c_action_server PRIVATE -T${THREADX_LINKER_SCRIPT} --nmagic --gc-sections -u app_main)
target_link_libraries(riscv64_threadx_c_action_server PRIVATE
    example_interfaces__nano_ros_c NanoRos::NanoRos threadx_platform)

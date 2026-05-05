cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_cpp_listener LANGUAGES C CXX ASM)

find_package(NanoRos CONFIG REQUIRED)
include(threadx-riscv64-support)

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

add_executable(riscv64_threadx_cpp_listener src/main.cpp ${THREADX_STARTUP_SOURCE})
target_include_directories(riscv64_threadx_cpp_listener PRIVATE ${THREADX_STARTUP_INCLUDES}
    "${CMAKE_CURRENT_BINARY_DIR}/include")
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")
target_link_options(riscv64_threadx_cpp_listener PRIVATE -T${THREADX_LINKER_SCRIPT} --nmagic --gc-sections -u app_main)
target_link_libraries(riscv64_threadx_cpp_listener PRIVATE
    std_msgs__nano_ros_cpp NanoRos::NanoRosCpp threadx_platform)

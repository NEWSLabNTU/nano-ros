cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_action_client LANGUAGES C CXX)

find_package(NanoRos CONFIG REQUIRED)
include(nuttx-support)

# Generate C++ bindings from package.xml
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

# Build NuttX kernel ELF with this C++ source
nuttx_build_example(nuttx_cpp_action_client
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    LINK_INTERFACES example_interfaces__nano_ros_cpp
)

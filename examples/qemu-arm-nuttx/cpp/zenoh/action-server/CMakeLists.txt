cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_action_server LANGUAGES C CXX)

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/nuttx-support.cmake")

# Generate C++ bindings from package.xml
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")

# Build NuttX kernel ELF with this C++ source
nuttx_build_example(nuttx_cpp_action_server
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    LINK_INTERFACES example_interfaces__nano_ros_cpp
    COMPILE_DEFS "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
                 "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)

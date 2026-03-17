cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_service_server LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/nuttx-platform.cmake")

# Generate C++ message bindings from package.xml
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")

nuttx_build_example(nuttx_cpp_service_server
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    INCLUDE_DIRS "${example_interfaces_INCLUDE_DIRS}"
                 "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
    COMPILE_DEFS "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
                 "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
add_dependencies(nuttx_cpp_service_server_build example_interfaces__nano_ros_cpp_gen)

cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_listener LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/nuttx-platform.cmake")

# Generate C++ message bindings (produces headers in build dir)
nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")

# Build NuttX kernel ELF with this C++ source
nuttx_build_example(nuttx_cpp_listener
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    INCLUDE_DIRS "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp/std_msgs"
                 "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
    COMPILE_DEFS "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
                 "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
add_dependencies(nuttx_cpp_listener_build std_msgs__nano_ros_cpp_gen)

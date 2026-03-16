cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_talker LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/nuttx-platform.cmake")

# Generate C++ message bindings (produces headers in build dir)
nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)

# Build NuttX kernel ELF with this C++ source
nuttx_build_example(nuttx_cpp_talker
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main.cpp"
    INCLUDE_DIRS "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp/std_msgs"
                 "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
)
add_dependencies(nuttx_cpp_talker_build std_msgs__nano_ros_cpp_gen)

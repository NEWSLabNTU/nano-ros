# Issue 2817 fixture: a C++ component library whose usage requirements reach
# past `<pkg>/include` and `<pkg>/src`, as a package that vendors code does.
#
# - its class header includes a header from `node/include`, a PUBLIC include
#   directory of the library;
# - its source calls a function of `cur_vendor`, a library it links PUBLIC.
#
# Both are stated AFTER the registration, as a package may. The registration is
# TYPED, so on the linux board it also builds the native carrier
# (`<pkg>__<exe>`, program `cur_probe`), which must compile and link with them.
cmake_minimum_required(VERSION 3.22)
project(cur_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

add_library(cur_vendor STATIC vendor/src/answer.cpp)
target_include_directories(cur_vendor PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/vendor/include")

nano_ros_auto_add_library(cur_lib STATIC src/Probe.cpp)
nros_components_register_node(cur_lib
    HEADER cur_pkg/Probe.hpp
    PLUGIN cur_pkg::Probe
    EXECUTABLE cur_probe
    TYPED
    SHAPE rclcpp)

target_include_directories(cur_lib PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/node/include")
target_link_libraries(cur_lib PUBLIC cur_vendor)

cmake_minimum_required(VERSION 3.20)
project(pkg_cpp_listener LANGUAGES CXX)

# Phase 123.A.10 — multi-package workspace demo.
# C++ listener counterpart to pkg_c_talker. Subscribes to /chatter
# via nano-ros's C++ API; demonstrates that C and C++ packages can
# share one installed nano-ros prefix (NanoRos::NanoRos +
# NanoRos::NanoRosCpp) plus one workspace-shared std_msgs codegen
# output (NANO_ROS_GEN_CACHE_DIR).

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG)

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs LANGUAGE CPP
    DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(pkg_cpp_listener src/main.cpp)
# Phase 123.A.10 — RMW before platform (see pkg_c_talker for why).
nano_ros_link_rmw(pkg_cpp_listener)
nano_ros_link_platform(pkg_cpp_listener)
target_link_libraries(pkg_cpp_listener PRIVATE
    std_msgs__nano_ros_cpp
    NanoRos::NanoRosCpp)
target_compile_definitions(pkg_cpp_listener PRIVATE NROS_CPP_STD=1)

install(TARGETS pkg_cpp_listener RUNTIME DESTINATION lib/${PROJECT_NAME})

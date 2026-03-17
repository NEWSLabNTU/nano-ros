cmake_minimum_required(VERSION 3.16)
project(cpp_listener
    VERSION 0.1.0
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ============================================================================
# nros C++ setup
# ============================================================================

find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Generate C++ bindings for std_msgs/Int32
# ============================================================================

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(cpp_listener src/main.cpp)
target_link_libraries(cpp_listener
    PRIVATE
        std_msgs__nano_ros_cpp
        NanoRos::NanoRosCpp
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS cpp_listener RUNTIME DESTINATION bin)

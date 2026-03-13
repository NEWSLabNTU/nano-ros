cmake_minimum_required(VERSION 3.16)
project(cpp_talker
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

nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    LANGUAGE CPP
    SKIP_INSTALL
)

# ============================================================================
# Executable
# ============================================================================

add_executable(cpp_talker src/main.cpp)
target_link_libraries(cpp_talker
    PRIVATE
        std_msgs__nano_ros_cpp
        NanoRos::NanoRosCpp
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS cpp_talker RUNTIME DESTINATION bin)

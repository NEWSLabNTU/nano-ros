cmake_minimum_required(VERSION 3.16)
project(dds_cpp_service_server
    VERSION 0.1.0
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ============================================================================
# nros C++ setup
# ============================================================================

set(NANO_ROS_RMW dds)
find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Generate C++ bindings for example_interfaces/srv/AddTwoInts
# ============================================================================

nros_generate_interfaces(example_interfaces LANGUAGE CPP SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(cpp_service_server src/main.cpp)
target_link_libraries(cpp_service_server
    PRIVATE
        example_interfaces__nano_ros_cpp
        NanoRos::NanoRosCpp
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS cpp_service_server RUNTIME DESTINATION bin)

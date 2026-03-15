cmake_minimum_required(VERSION 3.16)
project(cpp_action_client
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
# Generate C++ bindings for example_interfaces/action/Fibonacci
# ============================================================================

nano_ros_generate_interfaces(example_interfaces LANGUAGE CPP SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(cpp_action_client src/main.cpp)
target_link_libraries(cpp_action_client
    PRIVATE
        example_interfaces__nano_ros_cpp
        NanoRos::NanoRosCpp
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS cpp_action_client RUNTIME DESTINATION bin)

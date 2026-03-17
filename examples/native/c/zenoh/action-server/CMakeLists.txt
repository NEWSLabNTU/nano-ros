cmake_minimum_required(VERSION 3.16)
project(c_action_server
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nros-c setup
# ============================================================================

find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Generate C bindings for Fibonacci action
# ============================================================================

nros_generate_interfaces(example_interfaces SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(c_action_server src/main.c)
target_link_libraries(c_action_server
    PRIVATE
        example_interfaces__nano_ros_c
        NanoRos::NanoRos
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS c_action_server RUNTIME DESTINATION bin)

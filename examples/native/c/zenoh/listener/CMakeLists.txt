cmake_minimum_required(VERSION 3.16)
project(c_listener
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
# Generate C bindings for std_msgs/Int32
# ============================================================================

nano_ros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nano_ros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(c_listener src/main.c)
target_link_libraries(c_listener
    PRIVATE
        std_msgs__nano_ros_c
        NanoRos::NanoRos
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS c_listener RUNTIME DESTINATION bin)

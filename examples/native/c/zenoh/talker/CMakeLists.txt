cmake_minimum_required(VERSION 3.16)
project(c_talker
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

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

# ============================================================================
# Executable
# ============================================================================

add_executable(c_talker src/main.c)
target_link_libraries(c_talker
    PRIVATE
        std_msgs__nano_ros_c
        NanoRos::NanoRos
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS c_talker RUNTIME DESTINATION bin)

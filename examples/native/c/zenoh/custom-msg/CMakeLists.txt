cmake_minimum_required(VERSION 3.16)
project(native_c_custom_msg
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
# Generate C message bindings
# ============================================================================

nros_generate_interfaces(${PROJECT_NAME} SKIP_INSTALL)

# ============================================================================
# Test executable that uses generated messages
# ============================================================================

add_executable(test_messages src/main.c)

target_link_libraries(test_messages
    PRIVATE
        ${PROJECT_NAME}__nano_ros_c
        NanoRos::NanoRos
)

cmake_minimum_required(VERSION 3.16)
project(c_service_client
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nros-c setup
# ============================================================================

if(NOT DEFINED NANO_ROS_ROOT)
    message(FATAL_ERROR
        "NANO_ROS_ROOT not set.\n"
        "Please specify the path to your nros workspace:\n"
        "  cmake -DNANO_ROS_ROOT=/path/to/nros .."
    )
endif()

list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/cmake")
list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/packages/core/nros-c/cmake")
find_package(NanoRos REQUIRED)

# Include the generate interfaces function
include(nano_ros_generate_interfaces)

# ============================================================================
# Generate C bindings for AddTwoInts service
# ============================================================================

nano_ros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    SKIP_INSTALL
)

# ============================================================================
# Executable
# ============================================================================

add_executable(c_service_client src/main.c)
target_link_libraries(c_service_client
    PRIVATE
        example_interfaces__nano_ros_c
        NanoRos::NanoRos
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS c_service_client RUNTIME DESTINATION bin)

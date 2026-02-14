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

# Pass -DNANO_ROS_ROOT=/path/to/nros when configuring:
#   cmake -DNANO_ROS_ROOT=/path/to/nros ..
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
# Generate C message bindings
# ============================================================================

nano_ros_generate_interfaces(${PROJECT_NAME}
    "msg/Temperature.msg"
    "msg/SensorData.msg"
    SKIP_INSTALL  # Don't install for this example
)

# ============================================================================
# Test executable that uses generated messages
# ============================================================================

add_executable(test_messages src/main.c)

target_link_libraries(test_messages
    PRIVATE
        ${PROJECT_NAME}__nano_ros_c
        NanoRos::NanoRos
)

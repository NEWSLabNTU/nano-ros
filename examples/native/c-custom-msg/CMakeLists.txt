cmake_minimum_required(VERSION 3.16)
project(native_c_custom_msg
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nano-ros-c setup
# ============================================================================

# Pass -DNANO_ROS_ROOT=/path/to/nano-ros when configuring:
#   cmake -DNANO_ROS_ROOT=/path/to/nano-ros ..
if(NOT DEFINED NANO_ROS_ROOT)
    message(FATAL_ERROR
        "NANO_ROS_ROOT not set.\n"
        "Please specify the path to your nano-ros workspace:\n"
        "  cmake -DNANO_ROS_ROOT=/path/to/nano-ros .."
    )
endif()

# Add cmake module path for FindNanoRosC and nano_ros_generate_interfaces
list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/crates/nano-ros-c/cmake")

# Find nano-ros-c library
find_package(NanoRosC REQUIRED)

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
        nano_ros_c::nano_ros_c
)

# ============================================================================
# Print configuration info
# ============================================================================

message(STATUS "")
message(STATUS "native-c-custom-msg configuration:")
message(STATUS "  nano-ros-c include: ${NanoRosC_INCLUDE_DIRS}")
message(STATUS "  nano-ros-c library: ${NanoRosC_LIBRARIES}")
message(STATUS "  Generated headers: ${${PROJECT_NAME}_GENERATED_HEADERS}")
message(STATUS "  Generated sources: ${${PROJECT_NAME}_GENERATED_SOURCES}")
message(STATUS "")

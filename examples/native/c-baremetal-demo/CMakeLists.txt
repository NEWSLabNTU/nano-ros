cmake_minimum_required(VERSION 3.16)
project(baremetal_demo C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Find nano-ros-c library (built from Rust)
# The library should be pre-built with: cargo build --release -p nano-ros-c
#
# Pass -DNANO_ROS_ROOT=/path/to/nano-ros when configuring:
#   cmake -DNANO_ROS_ROOT=/path/to/nano-ros ..
if(NOT DEFINED NANO_ROS_ROOT)
    message(FATAL_ERROR
        "NANO_ROS_ROOT not set.\n"
        "Please specify the path to your nano-ros workspace:\n"
        "  cmake -DNANO_ROS_ROOT=/path/to/nano-ros .."
    )
endif()

set(NANO_ROS_C_INCLUDE "${NANO_ROS_ROOT}/crates/nano-ros-c/include")

# Look for the static library in common locations
find_library(NANO_ROS_C_LIB
    NAMES nano_ros_c libnano_ros_c
    PATHS
        "${NANO_ROS_ROOT}/target/release"
        "${NANO_ROS_ROOT}/target/debug"
    NO_DEFAULT_PATH
)

if(NOT NANO_ROS_C_LIB)
    message(FATAL_ERROR
        "Could not find libnano_ros_c. Please build it first:\n"
        "  cd ${NANO_ROS_ROOT} && cargo build --release -p nano-ros-c\n"
    )
endif()

message(STATUS "Found nano-ros-c library: ${NANO_ROS_C_LIB}")
message(STATUS "nano-ros-c include dir: ${NANO_ROS_C_INCLUDE}")

# Create executable
add_executable(baremetal_demo
    src/main.c
    src/platform_impl.c
)

# Include paths
target_include_directories(baremetal_demo PRIVATE
    ${NANO_ROS_C_INCLUDE}
)

# Link libraries
target_link_libraries(baremetal_demo
    ${NANO_ROS_C_LIB}
)

# Platform-specific libraries
if(UNIX AND NOT APPLE)
    # Linux
    target_link_libraries(baremetal_demo
        pthread
        dl
        m
        rt  # For clock_gettime
    )
elseif(APPLE)
    # macOS
    target_link_libraries(baremetal_demo
        pthread
        dl
        m
        "-framework CoreFoundation"
        "-framework Security"
    )
endif()

# Compiler warnings
target_compile_options(baremetal_demo PRIVATE
    -Wall
    -Wextra
    -Wpedantic
    -Wno-unused-result  # Demo intentionally ignores return values in cleanup paths
)

# Print build info
message(STATUS "")
message(STATUS "Bare-metal Demo Build Configuration:")
message(STATUS "  C Compiler: ${CMAKE_C_COMPILER}")
message(STATUS "  C Standard: C${CMAKE_C_STANDARD}")
message(STATUS "  Build Type: ${CMAKE_BUILD_TYPE}")
message(STATUS "")

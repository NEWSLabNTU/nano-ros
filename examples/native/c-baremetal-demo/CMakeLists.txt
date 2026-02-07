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

list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/cmake")
find_package(NanoRos REQUIRED)

# Create executable
add_executable(baremetal_demo
    src/main.c
    src/platform_impl.c
)

# Link libraries
target_link_libraries(baremetal_demo PRIVATE NanoRos::NanoRos)

# Platform-specific libraries not covered by FindNanoRos
if(UNIX AND NOT APPLE)
    target_link_libraries(baremetal_demo PRIVATE rt)
elseif(APPLE)
    target_link_libraries(baremetal_demo PRIVATE "-framework CoreFoundation")
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

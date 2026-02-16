cmake_minimum_required(VERSION 3.15)
project(c_xrce_listener
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nros-c dependency
# ============================================================================
# Build nros-c with XRCE features before configuring:
#   cargo build -p nros-c --release --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"
#
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
find_package(NanoRos REQUIRED)

# ============================================================================
# Executable
# ============================================================================
add_executable(c_xrce_listener src/main.c)
target_link_libraries(c_xrce_listener PRIVATE NanoRos::NanoRos)

# ============================================================================
# Installation (optional)
# ============================================================================
install(TARGETS c_xrce_listener RUNTIME DESTINATION bin)

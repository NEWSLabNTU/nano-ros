cmake_minimum_required(VERSION 3.15)
project(c_listener
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nano-ros-c dependency
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

# Include directory for nano_ros headers
set(NANO_ROS_C_INCLUDE "${NANO_ROS_ROOT}/crates/nano-ros-c/include")

# Library directory (workspace target, release build)
set(NANO_ROS_C_LIB_DIR "${NANO_ROS_ROOT}/target/release")

# Check if library exists
if(NOT EXISTS "${NANO_ROS_C_LIB_DIR}/libnano_ros_c.a")
    message(FATAL_ERROR
        "nano-ros-c library not found at ${NANO_ROS_C_LIB_DIR}/libnano_ros_c.a\n"
        "Please build nano-ros-c first:\n"
        "  cd ${NANO_ROS_ROOT} && cargo build -p nano-ros-c --release"
    )
endif()

# ============================================================================
# Executable
# ============================================================================
add_executable(c_listener src/main.c)

target_include_directories(c_listener
    PRIVATE
        ${NANO_ROS_C_INCLUDE}
)

# Link against nano-ros-c static library
target_link_libraries(c_listener
    PRIVATE
        "${NANO_ROS_C_LIB_DIR}/libnano_ros_c.a"
)

# Platform-specific libraries
if(UNIX AND NOT APPLE)
    target_link_libraries(c_listener PRIVATE pthread dl m)
elseif(APPLE)
    target_link_libraries(c_listener PRIVATE pthread dl m "-framework Security")
endif()

# ============================================================================
# Installation (optional)
# ============================================================================
install(TARGETS c_listener
    RUNTIME DESTINATION bin
)

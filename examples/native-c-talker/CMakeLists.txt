cmake_minimum_required(VERSION 3.15)
project(c_talker
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nano-ros-c dependency
# ============================================================================
# nano-ros workspace root
set(NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")

# Include directory for nano_ros.h
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
add_executable(c_talker src/main.c)

target_include_directories(c_talker
    PRIVATE
        ${NANO_ROS_C_INCLUDE}
)

# Link against nano-ros-c static library
target_link_libraries(c_talker
    PRIVATE
        "${NANO_ROS_C_LIB_DIR}/libnano_ros_c.a"
)

# Platform-specific libraries
if(UNIX AND NOT APPLE)
    target_link_libraries(c_talker PRIVATE pthread dl m)
elseif(APPLE)
    target_link_libraries(c_talker PRIVATE pthread dl m "-framework Security")
endif()

# ============================================================================
# Installation (optional)
# ============================================================================
install(TARGETS c_talker
    RUNTIME DESTINATION bin
)

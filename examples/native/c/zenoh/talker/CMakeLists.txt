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

# Auto-detect pseudo-install dir when building inside the repo
if(NOT DEFINED NanoRos_DIR)
    get_filename_component(_repo_root "${CMAKE_CURRENT_LIST_DIR}/../../../../.." ABSOLUTE)
    set(_local "${_repo_root}/build/install/lib/cmake/NanoRos")
    if(EXISTS "${_local}/NanoRosConfig.cmake")
        set(NanoRos_DIR "${_local}")
    endif()
endif()

find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Generate C bindings for std_msgs/Int32
# ============================================================================

nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    SKIP_INSTALL
)

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

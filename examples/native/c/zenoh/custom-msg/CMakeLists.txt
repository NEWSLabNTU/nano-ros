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

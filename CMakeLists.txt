cmake_minimum_required(VERSION 3.22)
project(NanoRos VERSION 0.1.0 LANGUAGES C)

# ---------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------
option(NANO_ROS_BUILD_CODEGEN "Build the nros-codegen tool" ON)
set(NANO_ROS_RMW "zenoh" CACHE STRING "RMW backend: zenoh, xrce, or dds")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS zenoh xrce dds)
set(NANO_ROS_PLATFORM "posix" CACHE STRING
    "Target platform: posix, freertos_armcm3, nuttx_armv7a")
set_property(CACHE NANO_ROS_PLATFORM PROPERTY STRINGS posix freertos_armcm3 nuttx_armv7a)

# ---------------------------------------------------------------------------
# Corrosion — integrates Rust/Cargo into CMake
# ---------------------------------------------------------------------------
include(FetchContent)
FetchContent_Declare(Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG        v0.6.1
)
FetchContent_MakeAvailable(Corrosion)

# ---------------------------------------------------------------------------
# Sub-projects
# ---------------------------------------------------------------------------
add_subdirectory(packages/core/nros-c)
add_subdirectory(packages/core/nros-cpp)

# codegen tool and bundled interfaces are host-only artifacts; skip them
# when cross-compiling for an embedded target.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    if(NANO_ROS_BUILD_CODEGEN)
        add_subdirectory(packages/codegen/packages/nros-codegen-c)
    endif()

    # Bundled interfaces (optional — codegen works via ament index when absent)
    if(IS_DIRECTORY "${PROJECT_SOURCE_DIR}/packages/codegen/interfaces")
        install(DIRECTORY packages/codegen/interfaces/
            DESTINATION share/nano-ros/interfaces
        )
    else()
        install(DIRECTORY DESTINATION share/nano-ros/interfaces)
    endif()
endif()

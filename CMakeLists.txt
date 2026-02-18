cmake_minimum_required(VERSION 3.22)
project(NanoRos VERSION 0.1.0 LANGUAGES C)

# ---------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------
option(NANO_ROS_BUILD_CODEGEN "Build the nros-codegen tool" ON)
set(NANO_ROS_RMW "zenoh" CACHE STRING "RMW backend: zenoh or xrce")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS zenoh xrce)

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

if(NANO_ROS_BUILD_CODEGEN)
    add_subdirectory(packages/codegen/packages/nros-codegen-c)
endif()

# ---------------------------------------------------------------------------
# Bundled interfaces (if present — optional, codegen works via ament index)
# ---------------------------------------------------------------------------
if(IS_DIRECTORY "${PROJECT_SOURCE_DIR}/packages/codegen/interfaces")
    install(DIRECTORY packages/codegen/interfaces/
        DESTINATION share/nano-ros/interfaces
    )
else()
    # Ensure the directory structure exists even if empty
    install(DIRECTORY DESTINATION share/nano-ros/interfaces)
endif()

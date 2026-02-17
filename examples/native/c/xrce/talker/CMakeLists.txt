cmake_minimum_required(VERSION 3.15)
project(c_xrce_talker
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
# The pseudo-install dir (build/install/) contains zenoh-flavored nros-c by
# default.  XRCE examples pass the library path explicitly via NROS_C_LIBRARY.

if(NOT DEFINED NanoRos_DIR)
    get_filename_component(_repo_root "${CMAKE_CURRENT_LIST_DIR}/../../../../.." ABSOLUTE)
    set(_local "${_repo_root}/build/install/lib/cmake/NanoRos")
    if(EXISTS "${_local}/NanoRosConfig.cmake")
        set(NanoRos_DIR "${_local}")
    endif()
endif()

find_package(NanoRos REQUIRED CONFIG)

# Override with XRCE-flavored library when provided
if(DEFINED NROS_C_LIBRARY AND EXISTS "${NROS_C_LIBRARY}")
    set_target_properties(NanoRos::NanoRos PROPERTIES
        IMPORTED_LOCATION "${NROS_C_LIBRARY}"
    )
endif()

# ============================================================================
# Executable
# ============================================================================
add_executable(c_xrce_talker src/main.c)
target_link_libraries(c_xrce_talker PRIVATE NanoRos::NanoRos)

# ============================================================================
# Installation (optional)
# ============================================================================
install(TARGETS c_xrce_talker RUNTIME DESTINATION bin)

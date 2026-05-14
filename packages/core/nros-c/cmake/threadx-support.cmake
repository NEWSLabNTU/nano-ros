# threadx-support.cmake
#
# Layer-3 cmake support module for ThreadX-Linux C/C++ examples.
# Phase 112.E: shipped via `find_package(NanoRos)` install layout.
#
# Networking goes through nsos-netx (NetX BSD compatibility shim that
# forwards `nx_bsd_*` to host POSIX sockets) — no real NetX Duo stack,
# no /dev/net/tun, no veth bridge.
#
# Required variables (env or -D):
#   THREADX_DIR          — ThreadX kernel source root
#   THREADX_CONFIG_DIR   — directory containing tx_user.h
#   NSOS_NETX_DIR        — nsos-netx shim source
#   THREADX_APP_DEFINE   — path to the example's app_define.c
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)
#   include(threadx-support)

include(nros-threadx)

nros_threadx_validate(REQUIRE NSOS_NETX_DIR THREADX_APP_DEFINE)

nros_threadx_build_kernel(PORT "linux/gnu")
nros_threadx_build_netstack_nsos(SHIM_DIR "${NSOS_NETX_DIR}")

get_filename_component(_TX_SUPPORT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_INSTALL_PREFIX "${_TX_SUPPORT_DIR}/../../.." ABSOLUTE)
if(NOT DEFINED NROS_PLATFORM_THREADX_SOURCE_DIR)
    get_filename_component(_NROS_REPO_ROOT "${_NROS_INSTALL_PREFIX}/../.." ABSOLUTE)
    set(NROS_PLATFORM_THREADX_SOURCE_DIR
        "${_NROS_REPO_ROOT}/packages/core/nros-platform-threadx")
endif()
if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(_NROS_REPO_ROOT "${_NROS_INSTALL_PREFIX}/../.." ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE
        "${_NROS_REPO_ROOT}/packages/core/nros-platform-cffi/include")
endif()
if(NOT EXISTS "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/platform.c")
    message(FATAL_ERROR
        "threadx-support: nros-platform-threadx sources not found at "
        "${NROS_PLATFORM_THREADX_SOURCE_DIR}. Pass "
        "-DNROS_PLATFORM_THREADX_SOURCE_DIR=<repo>/packages/core/nros-platform-threadx.")
endif()
add_library(nros_platform_threadx_linux STATIC
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/platform.c"
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/net.c"
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/timer.c")
target_include_directories(nros_platform_threadx_linux PUBLIC
    "${NROS_PLATFORM_CFFI_INCLUDE}"
    "${NSOS_NETX_DIR}/include"
    ${NROS_THREADX_INCLUDES})
target_compile_definitions(nros_platform_threadx_linux PUBLIC ${NROS_THREADX_DEFINES})
target_link_libraries(nros_platform_threadx_linux PUBLIC nsos_netx threadx_kernel pthread)

# Phase 112.E.fix — app_define.c is NOT built into a STATIC lib here
# because its `nros_platform_threadx_*` undef refs can't be resolved
# from `NanoRos::NanoRos` (or `NanoRos::NanoRosCpp`) when the archive
# member is extracted *after* those libs in the linker command line.
# Export the source path so each example adds it to `add_executable`
# directly — undef refs are visible from the outset and the static
# libs further right on the link line satisfy them on first pass.
set(THREADX_APP_DEFINE_SOURCE "${THREADX_APP_DEFINE}" CACHE INTERNAL "")
set(THREADX_GLUE_DEFINES ${NROS_THREADX_DEFINES} CACHE INTERNAL "")
nros_threadx_compose_platform(
    COMPONENTS nros_platform_threadx_linux nsos_netx threadx_kernel
    LINK_LIBS pthread)

# Startup source ships under share/nano_ros/platform/threadx-linux/.
set(THREADX_STARTUP_SOURCE
    "${_NROS_INSTALL_PREFIX}/share/nano_ros/platform/threadx-linux/startup.c")
if(NOT EXISTS "${THREADX_STARTUP_SOURCE}")
    message(FATAL_ERROR
        "threadx-support: startup.c not found at ${THREADX_STARTUP_SOURCE}. "
        "Reinstall NanoRos (`just threadx-linux install`).")
endif()
set(THREADX_STARTUP_INCLUDES "${NROS_THREADX_INCLUDES}")

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
nros_threadx_build_glue(SOURCES "${THREADX_APP_DEFINE}")
nros_threadx_compose_platform(LINK_LIBS pthread)

# Startup source ships under share/nano_ros/platform/threadx-linux/.
get_filename_component(_TX_SUPPORT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_INSTALL_PREFIX "${_TX_SUPPORT_DIR}/../../.." ABSOLUTE)
set(THREADX_STARTUP_SOURCE
    "${_NROS_INSTALL_PREFIX}/share/nano_ros/platform/threadx-linux/startup.c")
if(NOT EXISTS "${THREADX_STARTUP_SOURCE}")
    message(FATAL_ERROR
        "threadx-support: startup.c not found at ${THREADX_STARTUP_SOURCE}. "
        "Reinstall NanoRos (`just threadx-linux install`).")
endif()
set(THREADX_STARTUP_INCLUDES "${NROS_THREADX_INCLUDES}")

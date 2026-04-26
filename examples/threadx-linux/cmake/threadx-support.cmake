# threadx-support.cmake
#
# CMake support module for ThreadX Linux C/C++ examples (layer 3).
# Phase 91.E1a: thin orchestrator on top of `nros-threadx.cmake`,
# which is shipped via the cmake install (find_package(NanoRos)).
#
# Provides the `threadx_platform` INTERFACE target and exports
# THREADX_STARTUP_SOURCE / THREADX_STARTUP_INCLUDES so per-example
# CMakeLists.txt files can link their executables.
#
# Networking goes through nsos-netx (NetX BSD compatibility shim that
# forwards `nx_bsd_*` to host POSIX sockets) — no real NetX Duo stack,
# no /dev/net/tun, no veth bridge.
#
# Required variables (pass via cmake -D or environment):
#   THREADX_DIR          — ThreadX kernel source root
#   THREADX_CONFIG_DIR   — directory containing tx_user.h
#   NSOS_NETX_DIR        — nsos-netx shim source
#                          (packages/drivers/nsos-netx)
#   THREADX_APP_DEFINE   — path to the example's app_define.c
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)

include(nros-threadx)

nros_threadx_validate(REQUIRE NSOS_NETX_DIR THREADX_APP_DEFINE)

nros_threadx_build_kernel(PORT "linux/gnu")
nros_threadx_build_netstack_nsos(SHIM_DIR "${NSOS_NETX_DIR}")
nros_threadx_build_glue(SOURCES "${THREADX_APP_DEFINE}")
nros_threadx_compose_platform(LINK_LIBS pthread)

# Startup source ships next to this support file (layer 3 only).
get_filename_component(_TX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(THREADX_STARTUP_SOURCE "${_TX_CMAKE_DIR}/startup.c")
set(THREADX_STARTUP_INCLUDES "${NROS_THREADX_INCLUDES}")

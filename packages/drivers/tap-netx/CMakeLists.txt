cmake_minimum_required(VERSION 3.16)
project(tap-netx C)

# TAP-based network driver for NetX Duo on Linux.
# Requires NETX_DIR and THREADX_DIR to point to NetX Duo and ThreadX sources.

if(NOT DEFINED NETX_DIR)
    message(FATAL_ERROR "NETX_DIR must be set to the NetX Duo source directory")
endif()

if(NOT DEFINED THREADX_DIR)
    message(FATAL_ERROR "THREADX_DIR must be set to the ThreadX kernel source directory")
endif()

add_library(tap-netx STATIC src/nx_tap_network_driver.c)

target_include_directories(tap-netx PUBLIC include)
target_include_directories(tap-netx PRIVATE
    ${NETX_DIR}/common/inc
    ${NETX_DIR}/ports/linux/gnu/inc
    ${THREADX_DIR}/common/inc
    ${THREADX_DIR}/ports/linux/gnu/inc
)

# Pass through user config headers if provided
if(DEFINED THREADX_CONFIG_DIR)
    target_include_directories(tap-netx PRIVATE ${THREADX_CONFIG_DIR})
    target_compile_definitions(tap-netx PRIVATE TX_INCLUDE_USER_DEFINE_FILE)
endif()

if(DEFINED NETX_CONFIG_DIR)
    target_include_directories(tap-netx PRIVATE ${NETX_CONFIG_DIR})
    target_compile_definitions(tap-netx PRIVATE NX_INCLUDE_USER_DEFINE_FILE)
endif()

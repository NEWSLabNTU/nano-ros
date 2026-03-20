# threadx-support.cmake
#
# Shared CMake support module for ThreadX Linux C/C++ examples.
#
# Provides:
#   threadx_platform   — static library (ThreadX kernel + NetX Duo + Linux TAP driver + glue)
#
# Does NOT provide NanoRos::NanoRos, NanoRos::NanoRosCpp, or codegen.
# The caller must first call:
#   find_package(NanoRos CONFIG REQUIRED)
#
# Required variables (pass via cmake -D or environment):
#   THREADX_DIR            — ThreadX kernel source root
#   NETX_DIR               — NetX Duo source root
#   THREADX_SAMPLES_DIR    — ThreadX learn-samples (for network driver)
#   THREADX_CONFIG_DIR     — Directory containing tx_user.h and nx_user.h

# ---- Validate required variables ----
foreach(_var THREADX_DIR NETX_DIR THREADX_SAMPLES_DIR THREADX_CONFIG_DIR)
    if(NOT DEFINED ${_var})
        if(DEFINED ENV{${_var}})
            set(${_var} "$ENV{${_var}}")
        else()
            message(FATAL_ERROR "${_var} not set. Pass -D${_var}=<path> or export ${_var}.")
        endif()
    endif()
endforeach()

set(_TX_PORT_DIR "${THREADX_DIR}/ports/linux/gnu")

# ---- Shared include directories ----
set(_TX_INCLUDES
    "${THREADX_CONFIG_DIR}"
    "${THREADX_DIR}/common/inc"
    "${_TX_PORT_DIR}/inc"
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/ports/linux/gnu/inc"
    "${NETX_DIR}/addons/BSD"
)

# ---- ThreadX kernel library ----
file(GLOB _tx_kernel_srcs "${THREADX_DIR}/common/src/*.c")
file(GLOB _tx_port_srcs "${_TX_PORT_DIR}/src/*.c")

add_library(threadx_kernel STATIC ${_tx_kernel_srcs} ${_tx_port_srcs})
target_include_directories(threadx_kernel PRIVATE ${_TX_INCLUDES})
target_compile_definitions(threadx_kernel PRIVATE
    TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(threadx_kernel PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(threadx_kernel PROPERTIES C_STANDARD 11)

# ---- NetX Duo library ----
file(GLOB _netx_srcs "${NETX_DIR}/common/src/*.c")
add_library(netxduo STATIC ${_netx_srcs} "${NETX_DIR}/addons/BSD/nxd_bsd.c")
target_include_directories(netxduo PRIVATE ${_TX_INCLUDES})
target_compile_definitions(netxduo PRIVATE
    TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(netxduo PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(netxduo PROPERTIES C_STANDARD 11)

# ---- Linux network driver ----
set(_driver_src "${THREADX_SAMPLES_DIR}/courses/netxduo/Driver/nx_linux_network_driver.c")
add_library(netxdriver STATIC "${_driver_src}")
target_include_directories(netxdriver PRIVATE ${_TX_INCLUDES})
target_compile_definitions(netxdriver PRIVATE
    TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(netxdriver PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(netxdriver PROPERTIES C_STANDARD 11)

# ---- Board glue (app_define.c) ----
# THREADX_APP_DEFINE must be passed by the build script (path to app_define.c).
if(NOT DEFINED THREADX_APP_DEFINE)
    if(DEFINED ENV{THREADX_APP_DEFINE})
        set(THREADX_APP_DEFINE "$ENV{THREADX_APP_DEFINE}")
    else()
        message(FATAL_ERROR
            "THREADX_APP_DEFINE not set. Pass -DTHREADX_APP_DEFINE=<path> "
            "pointing to your app_define.c.")
    endif()
endif()

add_library(threadx_glue STATIC "${THREADX_APP_DEFINE}")
target_include_directories(threadx_glue PRIVATE ${_TX_INCLUDES})
target_compile_definitions(threadx_glue PRIVATE
    TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(threadx_glue PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(threadx_glue PROPERTIES C_STANDARD 11)

# ---- Combined platform target ----
add_library(threadx_platform INTERFACE)
target_link_libraries(threadx_platform INTERFACE
    threadx_glue netxdriver netxduo threadx_kernel pthread)
target_include_directories(threadx_platform INTERFACE ${_TX_INCLUDES})

# ---- Startup source (provides main() → tx_kernel_enter()) ----
# Each example links this source file into its executable.
get_filename_component(_TX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(THREADX_STARTUP_SOURCE "${_TX_CMAKE_DIR}/startup.c")
set(THREADX_STARTUP_INCLUDES ${_TX_INCLUDES})

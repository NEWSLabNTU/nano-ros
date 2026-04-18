# threadx-support.cmake
#
# Shared CMake support module for ThreadX Linux C/C++ examples.
#
# Provides:
#   threadx_platform   — static library (ThreadX kernel + nsos-netx BSD shim + glue)
#
# Networking goes through nsos-netx (NetX BSD compatibility shim that
# forwards `nx_bsd_*` to host POSIX sockets) — no NetX Duo TCP/IP stack,
# no /dev/net/tun, no veth bridge required.
#
# Does NOT provide NanoRos::NanoRos, NanoRos::NanoRosCpp, or codegen.
# The caller must first call:
#   find_package(NanoRos CONFIG REQUIRED)
#
# Required variables (pass via cmake -D or environment):
#   THREADX_DIR            — ThreadX kernel source root
#   THREADX_CONFIG_DIR     — Directory containing tx_user.h
#   NSOS_NETX_DIR          — nsos-netx shim source (packages/drivers/nsos-netx)

# ---- Validate required variables ----
foreach(_var THREADX_DIR THREADX_CONFIG_DIR NSOS_NETX_DIR)
    if(NOT DEFINED ${_var})
        if(DEFINED ENV{${_var}})
            set(${_var} "$ENV{${_var}}")
        else()
            message(FATAL_ERROR "${_var} not set. Pass -D${_var}=<path> or export ${_var}.")
        endif()
    endif()
endforeach()

set(_TX_PORT_DIR "${THREADX_DIR}/ports/linux/gnu")

# ---- ThreadX include directories ----
set(_TX_INCLUDES
    "${THREADX_CONFIG_DIR}"
    "${THREADX_DIR}/common/inc"
    "${_TX_PORT_DIR}/inc"
)

# ---- ThreadX kernel library ----
file(GLOB _tx_kernel_srcs "${THREADX_DIR}/common/src/*.c")
file(GLOB _tx_port_srcs "${_TX_PORT_DIR}/src/*.c")

add_library(threadx_kernel STATIC ${_tx_kernel_srcs} ${_tx_port_srcs})
target_include_directories(threadx_kernel PRIVATE ${_TX_INCLUDES})
target_compile_definitions(threadx_kernel PRIVATE TX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(threadx_kernel PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(threadx_kernel PROPERTIES C_STANDARD 11)

# ---- nsos-netx (NetX BSD compatibility shim over POSIX) ----
add_library(nsos_netx STATIC "${NSOS_NETX_DIR}/src/nsos_netx.c")
target_include_directories(nsos_netx PUBLIC "${NSOS_NETX_DIR}/include")
target_compile_options(nsos_netx PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(nsos_netx PROPERTIES C_STANDARD 11)

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
target_compile_definitions(threadx_glue PRIVATE TX_INCLUDE_USER_DEFINE_FILE)
target_compile_options(threadx_glue PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(threadx_glue PROPERTIES C_STANDARD 11)

# ---- Combined platform target ----
add_library(threadx_platform INTERFACE)
target_link_libraries(threadx_platform INTERFACE
    threadx_glue nsos_netx threadx_kernel pthread)
target_include_directories(threadx_platform INTERFACE ${_TX_INCLUDES})

# ---- Startup source (provides main() → tx_kernel_enter()) ----
# Each example links this source file into its executable.
get_filename_component(_TX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(THREADX_STARTUP_SOURCE "${_TX_CMAKE_DIR}/startup.c")
set(THREADX_STARTUP_INCLUDES ${_TX_INCLUDES})

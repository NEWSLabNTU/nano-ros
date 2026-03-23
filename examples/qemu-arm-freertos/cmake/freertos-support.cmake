# freertos-support.cmake
#
# Shared CMake support module for FreeRTOS MPS2-AN385 C/C++ examples.
#
# Provides:
#   freertos_platform    — static library (FreeRTOS + lwIP + LAN9118 + startup)
#   FREERTOS_STARTUP_SOURCE    — startup.c (compiled per-example, see below)
#   FREERTOS_STARTUP_INCLUDES  — include dirs needed by startup.c
#
# Does NOT provide NanoRos::NanoRos, NanoRos::NanoRosCpp, or codegen.
# The caller must first call:
#   find_package(NanoRos CONFIG REQUIRED)
#
# Required variables (set via environment or cmake -D):
#   FREERTOS_DIR     — FreeRTOS kernel source root
#   LWIP_DIR         — lwIP source root
#
# Optional:
#   FREERTOS_PORT    — portable layer (default: GCC/ARM_CM3)

# ---- Resolve paths ----
get_filename_component(_FREERTOS_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_ROOT "${_FREERTOS_CMAKE_DIR}/../../.." ABSOLUTE)
set(_BOARD_CONFIG_DIR "${_NROS_ROOT}/packages/boards/nros-mps2-an385-freertos/config")
set(_LAN9118_DIR "${_NROS_ROOT}/packages/drivers/lan9118-lwip")

# ---- Environment variables ----
if(NOT DEFINED FREERTOS_DIR)
    if(DEFINED ENV{FREERTOS_DIR})
        set(FREERTOS_DIR "$ENV{FREERTOS_DIR}")
    else()
        set(FREERTOS_DIR "${_NROS_ROOT}/third-party/freertos/kernel")
    endif()
endif()
if(NOT DEFINED LWIP_DIR)
    if(DEFINED ENV{LWIP_DIR})
        set(LWIP_DIR "$ENV{LWIP_DIR}")
    else()
        set(LWIP_DIR "${_NROS_ROOT}/third-party/freertos/lwip")
    endif()
endif()
if(NOT DEFINED FREERTOS_PORT)
    if(DEFINED ENV{FREERTOS_PORT})
        set(FREERTOS_PORT "$ENV{FREERTOS_PORT}")
    else()
        set(FREERTOS_PORT "GCC/ARM_CM3")
    endif()
endif()

set(_FREERTOS_PORT_DIR "${FREERTOS_DIR}/portable/${FREERTOS_PORT}")

# ---- Common include directories ----
set(_FREERTOS_INCLUDES
    "${_BOARD_CONFIG_DIR}"
    "${FREERTOS_DIR}/include"
    "${_FREERTOS_PORT_DIR}"
)
set(_LWIP_INCLUDES
    "${LWIP_DIR}/src/include"
    "${LWIP_DIR}/contrib/ports/freertos/include"
)

# ============================================================================
# FreeRTOS kernel
# ============================================================================
add_library(freertos_kernel STATIC
    "${FREERTOS_DIR}/tasks.c"
    "${FREERTOS_DIR}/queue.c"
    "${FREERTOS_DIR}/list.c"
    "${FREERTOS_DIR}/timers.c"
    "${FREERTOS_DIR}/event_groups.c"
    "${FREERTOS_DIR}/stream_buffer.c"
    "${_FREERTOS_PORT_DIR}/port.c"
    "${FREERTOS_DIR}/portable/MemMang/heap_4.c"
)
target_include_directories(freertos_kernel PUBLIC ${_FREERTOS_INCLUDES})
target_compile_options(freertos_kernel PRIVATE -O2 -w)

# ============================================================================
# lwIP
# ============================================================================
add_library(lwip STATIC
    # Core
    "${LWIP_DIR}/src/core/init.c"
    "${LWIP_DIR}/src/core/def.c"
    "${LWIP_DIR}/src/core/dns.c"
    "${LWIP_DIR}/src/core/inet_chksum.c"
    "${LWIP_DIR}/src/core/ip.c"
    "${LWIP_DIR}/src/core/mem.c"
    "${LWIP_DIR}/src/core/memp.c"
    "${LWIP_DIR}/src/core/netif.c"
    "${LWIP_DIR}/src/core/pbuf.c"
    "${LWIP_DIR}/src/core/raw.c"
    "${LWIP_DIR}/src/core/stats.c"
    "${LWIP_DIR}/src/core/sys.c"
    "${LWIP_DIR}/src/core/tcp.c"
    "${LWIP_DIR}/src/core/tcp_in.c"
    "${LWIP_DIR}/src/core/tcp_out.c"
    "${LWIP_DIR}/src/core/timeouts.c"
    "${LWIP_DIR}/src/core/udp.c"
    # IPv4
    "${LWIP_DIR}/src/core/ipv4/etharp.c"
    "${LWIP_DIR}/src/core/ipv4/icmp.c"
    "${LWIP_DIR}/src/core/ipv4/ip4.c"
    "${LWIP_DIR}/src/core/ipv4/ip4_addr.c"
    "${LWIP_DIR}/src/core/ipv4/ip4_frag.c"
    # API (sockets)
    "${LWIP_DIR}/src/api/api_lib.c"
    "${LWIP_DIR}/src/api/api_msg.c"
    "${LWIP_DIR}/src/api/err.c"
    "${LWIP_DIR}/src/api/if_api.c"
    "${LWIP_DIR}/src/api/netbuf.c"
    "${LWIP_DIR}/src/api/netdb.c"
    "${LWIP_DIR}/src/api/netifapi.c"
    "${LWIP_DIR}/src/api/sockets.c"
    "${LWIP_DIR}/src/api/tcpip.c"
    # Netif
    "${LWIP_DIR}/src/netif/ethernet.c"
    # FreeRTOS sys_arch
    "${LWIP_DIR}/contrib/ports/freertos/sys_arch.c"
)
target_include_directories(lwip PUBLIC ${_FREERTOS_INCLUDES} ${_LWIP_INCLUDES})
target_compile_options(lwip PRIVATE -O2 -w)

# ============================================================================
# LAN9118 lwIP netif driver
# ============================================================================
add_library(lan9118_lwip STATIC
    "${_LAN9118_DIR}/src/lan9118_lwip.c"
)
target_include_directories(lan9118_lwip PUBLIC
    "${_LAN9118_DIR}/include"
    ${_FREERTOS_INCLUDES}
    ${_LWIP_INCLUDES}
)
target_compile_options(lan9118_lwip PRIVATE -O2 -w)

# ============================================================================
# Combined platform target
# ============================================================================
#
# Startup code (startup.c) is NOT compiled as a shared library because it
# uses preprocessor defines (APP_IP, APP_MAC, etc.) that differ per example.
# Instead, FREERTOS_STARTUP_SOURCE is exported so each example compiles it
# as part of its own executable (inheriting the example's compile definitions).
set(FREERTOS_STARTUP_SOURCE "${_FREERTOS_CMAKE_DIR}/startup.c" CACHE INTERNAL "")
set(FREERTOS_STARTUP_INCLUDES
    ${_FREERTOS_INCLUDES} ${_LWIP_INCLUDES} "${_LAN9118_DIR}/include"
    CACHE INTERNAL "")

add_library(freertos_platform INTERFACE)
target_link_libraries(freertos_platform INTERFACE
    lan9118_lwip lwip freertos_kernel
)

# Linker script
set(FREERTOS_LINKER_SCRIPT "${_BOARD_CONFIG_DIR}/mps2_an385.ld" CACHE INTERNAL "")
target_link_options(freertos_platform INTERFACE
    "-T${FREERTOS_LINKER_SCRIPT}"
    "-Wl,--gc-sections"
    "-nostartfiles"
    # nosys.specs ensures --start-group ordering: -lgcc -lc -lnosys --end-group
    # This is required for correct symbol resolution when Rust static libs are
    # present — manual -lc -lnosys ordering does not work reliably in that case.
    "--specs=nosys.specs"
)

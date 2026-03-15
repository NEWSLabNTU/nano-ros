# freertos-platform.cmake
#
# Shared CMake module for FreeRTOS MPS2-AN385 C/C++ examples.
#
# Provides:
#   freertos_platform    — static library (FreeRTOS + lwIP + LAN9118 + startup)
#   NanoRos::NanoRosCpp  — nros C++ API (header-only + FFI static lib)
#   nano_ros_generate_interfaces()  — codegen function
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
        set(FREERTOS_DIR "${_NROS_ROOT}/external/freertos-kernel")
    endif()
endif()
if(NOT DEFINED LWIP_DIR)
    if(DEFINED ENV{LWIP_DIR})
        set(LWIP_DIR "$ENV{LWIP_DIR}")
    else()
        set(LWIP_DIR "${_NROS_ROOT}/external/lwip")
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
# Startup + platform entry (vector table, Reset_Handler, network init, etc.)
# ============================================================================
add_library(freertos_startup STATIC
    "${_FREERTOS_CMAKE_DIR}/startup.c"
)
target_include_directories(freertos_startup PUBLIC
    ${_FREERTOS_INCLUDES}
    ${_LWIP_INCLUDES}
    "${_LAN9118_DIR}/include"
)
target_compile_options(freertos_startup PRIVATE -O2 -w)

# ============================================================================
# Combined platform target
# ============================================================================
add_library(freertos_platform INTERFACE)
target_link_libraries(freertos_platform INTERFACE
    freertos_startup lan9118_lwip lwip freertos_kernel
    c nosys gcc
)

# Linker script
set(FREERTOS_LINKER_SCRIPT "${_BOARD_CONFIG_DIR}/mps2_an385.ld" CACHE INTERNAL "")
target_link_options(freertos_platform INTERFACE
    "-T${FREERTOS_LINKER_SCRIPT}"
    "-Wl,--gc-sections"
)

# Newlib library search paths (multilib-correct)
execute_process(
    COMMAND arm-none-eabi-gcc -mcpu=cortex-m3 -mthumb --print-file-name=libc.a
    OUTPUT_VARIABLE _LIBC_PATH OUTPUT_STRIP_TRAILING_WHITESPACE
)
get_filename_component(_LIBC_DIR "${_LIBC_PATH}" DIRECTORY)
execute_process(
    COMMAND arm-none-eabi-gcc -mcpu=cortex-m3 -mthumb --print-file-name=libgcc.a
    OUTPUT_VARIABLE _LIBGCC_PATH OUTPUT_STRIP_TRAILING_WHITESPACE
)
get_filename_component(_LIBGCC_DIR "${_LIBGCC_PATH}" DIRECTORY)
target_link_directories(freertos_platform INTERFACE "${_LIBC_DIR}" "${_LIBGCC_DIR}")

# ============================================================================
# Corrosion — build nros Rust crates for ARM
# ============================================================================
include(FetchContent)
FetchContent_Declare(Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG        v0.6.1
)
FetchContent_MakeAvailable(Corrosion)

# Build nros-c and nros-cpp from the main workspace.
# These are built as staticlibs directly — NOT as dependencies of the
# panic-handler crate, because bare-metal staticlibs each require their
# own panic handler during compilation.
corrosion_import_crate(
    MANIFEST_PATH "${_NROS_ROOT}/Cargo.toml"
    CRATES        nros-c nros-cpp
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      rmw-zenoh platform-freertos ros-humble panic-halt
    LOCKED
)

# Pass FreeRTOS/lwIP paths and executor sizing to Cargo build
foreach(_tgt nros_c-static nros_cpp-static)
    corrosion_set_env_vars(${_tgt}
        "FREERTOS_DIR=${FREERTOS_DIR}"
        "LWIP_DIR=${LWIP_DIR}"
        "FREERTOS_PORT=${FREERTOS_PORT}"
        "FREERTOS_CONFIG_DIR=${_BOARD_CONFIG_DIR}"
        "NROS_EXECUTOR_MAX_CBS=4"
        "NROS_EXECUTOR_ARENA_SIZE=4096"
    )
endforeach()

# ---- NanoRos::NanoRos target (C API, cross-compiled) ----
add_library(NanoRosC INTERFACE)
add_library(NanoRos::NanoRos ALIAS NanoRosC)
target_include_directories(NanoRosC INTERFACE
    "${_NROS_ROOT}/packages/core/nros-c/include"
)
target_link_libraries(NanoRosC INTERFACE nros_c-static)

# ---- NanoRos::NanoRosCpp target (C++ API, cross-compiled) ----
add_library(NanoRosCpp INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS NanoRosCpp)
target_include_directories(NanoRosCpp INTERFACE
    "${_NROS_ROOT}/packages/core/nros-cpp/include"
)
target_link_libraries(NanoRosCpp INTERFACE nros_cpp-static)
target_compile_features(NanoRosCpp INTERFACE cxx_std_14)

# ============================================================================
# Codegen — nano_ros_generate_interfaces()
# ============================================================================

# Find or build nros-codegen (HOST tool)
set(_CODEGEN_CRATE "${_NROS_ROOT}/packages/codegen/packages/nros-codegen-c")
set(_CODEGEN_TARGET_DIR "${_NROS_ROOT}/packages/codegen/packages/target")
find_program(_NANO_ROS_CODEGEN_TOOL nros-codegen
    PATHS "${_CODEGEN_TARGET_DIR}/release" "${_CODEGEN_TARGET_DIR}/debug"
          "${_NROS_ROOT}/target/release" "${_NROS_ROOT}/target/debug"
    NO_DEFAULT_PATH
)
if(NOT _NANO_ROS_CODEGEN_TOOL)
    message(STATUS "nros-codegen not found, building...")
    execute_process(
        COMMAND cargo build --manifest-path "${_CODEGEN_CRATE}/Cargo.toml" --release
        WORKING_DIRECTORY "${_NROS_ROOT}"
        RESULT_VARIABLE _codegen_result
    )
    if(NOT _codegen_result EQUAL 0)
        message(FATAL_ERROR "Failed to build nros-codegen")
    endif()
    set(_NANO_ROS_CODEGEN_TOOL "${_CODEGEN_TARGET_DIR}/release/nros-codegen")
endif()
set(_NANO_ROS_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}" CACHE INTERNAL "Path to nros codegen tool")
message(STATUS "Found nros codegen tool: ${_NANO_ROS_CODEGEN_TOOL}")

# Set up paths so NanoRosGenerateInterfaces.cmake finds nros-serdes and
# bundled interfaces from the source tree (not an install prefix).
# The codegen cmake derives _NANO_ROS_PREFIX from its own file location,
# so we override it before including.
set(_NANO_ROS_CMAKE_DIR "${_NROS_ROOT}/packages/codegen/packages/nros-codegen-c/cmake")

# Pre-set _NANO_ROS_PREFIX so the codegen cmake uses the project root
set(_NANO_ROS_PREFIX "${_NROS_ROOT}")

# Create symlinks from share/ layout expected by codegen to source tree
if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes/src")
    set(_serdes_src "${_NROS_ROOT}/packages/core/nros-serdes")
    file(MAKE_DIRECTORY "${_NROS_ROOT}/share/nano-ros/rust")
    if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes")
        file(CREATE_LINK "${_serdes_src}" "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes" SYMBOLIC)
    endif()
endif()
if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/interfaces")
    file(CREATE_LINK "${_NROS_ROOT}/packages/codegen/interfaces"
         "${_NROS_ROOT}/share/nano-ros/interfaces" SYMBOLIC)
endif()

include("${_NANO_ROS_CMAKE_DIR}/NanoRosGenerateInterfaces.cmake")

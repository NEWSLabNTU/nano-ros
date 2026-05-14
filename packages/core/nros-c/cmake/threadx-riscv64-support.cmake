# threadx-riscv64-support.cmake
#
# Layer-3 cmake support module for ThreadX RISC-V 64-bit (QEMU virt)
# C/C++ examples. Phase 112.E: shipped via `find_package(NanoRos)`
# install layout.
#
# Provides the `threadx_platform` INTERFACE target plus
# `threadx_riscv64_strip_builtins(<archive>)` for codegen-emitted Rust
# archives that need their soft-float compiler_builtins members
# stripped before linking. Exports THREADX_LINKER_SCRIPT,
# THREADX_STARTUP_SOURCE, and THREADX_STARTUP_INCLUDES.
#
# Required variables (env or -D):
#   THREADX_DIR        — ThreadX kernel source root
#   NETX_DIR           — NetX Duo source root
#   THREADX_CONFIG_DIR — directory with tx_user.h, nx_user.h, link.lds
#   THREADX_BOARD_DIR  — board crate C dir (entry.s, app_define.c, …)
#   VIRTIO_DRIVER_DIR  — virtio-net-netx driver source dir
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)
#   include(threadx-riscv64-support)

include(nros-threadx)

nros_threadx_validate(REQUIRE NETX_DIR THREADX_BOARD_DIR VIRTIO_DRIVER_DIR)

# Resolve shipped asset paths.
get_filename_component(_TX_RV64_SUPPORT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_INSTALL_PREFIX "${_TX_RV64_SUPPORT_DIR}/../../.." ABSOLUTE)
set(_TX_RV64_SHARE "${_NROS_INSTALL_PREFIX}/share/nano_ros/platform/threadx-riscv64")

# picolibc must be set up *before* nros_threadx_build_kernel so the
# kernel/netxduo/virtio-net STATIC libs see picolibc headers via the
# global -isystem we install on CMAKE_C_FLAGS / CMAKE_CXX_FLAGS.
nros_threadx_setup_picolibc(CXX_COMPAT_DIR "${_TX_RV64_SHARE}/cxx-compat")

nros_threadx_build_kernel(
    PORT          "risc-v64/gnu"
    BOARD_DIR     "${THREADX_BOARD_DIR}"
    BOARD_OVERRIDES
        tx_initialize_low_level.S
        tx_thread_schedule.S
        tx_thread_context_save.S
        tx_thread_context_restore.S
        tx_thread_stack_build.S
        tx_thread_system_return.S
    QEMU_VIRT_DIR "${THREADX_DIR}/ports/risc-v64/gnu/example_build/qemu_virt"
    QEMU_VIRT_EXCLUDE
        trap.c
        hwtimer.c
        demo_threadx.c
    EXTRA_INCLUDES "${NETX_DIR}/common/inc"
                   "${NETX_DIR}/addons/BSD"
                   "${VIRTIO_DRIVER_DIR}/include"
    EXTRA_DEFINES NX_INCLUDE_USER_DEFINE_FILE NROS_PLATFORM_BAREMETAL)

nros_threadx_build_netstack_netxduo(
    NETX_DIR      "${NETX_DIR}"
    DRIVER_DIR    "${VIRTIO_DRIVER_DIR}"
    EXTRA_DEFINES NROS_PLATFORM_BAREMETAL)

nros_threadx_setup_rust_lld()

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
        "threadx-riscv64-support: nros-platform-threadx sources not found at "
        "${NROS_PLATFORM_THREADX_SOURCE_DIR}. Pass "
        "-DNROS_PLATFORM_THREADX_SOURCE_DIR=<repo>/packages/core/nros-platform-threadx.")
endif()

add_library(nros_platform_threadx_riscv64 STATIC
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/platform.c"
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/net.c"
    "${NROS_PLATFORM_THREADX_SOURCE_DIR}/src/timer.c")
target_include_directories(nros_platform_threadx_riscv64 PUBLIC
    "${NROS_PLATFORM_CFFI_INCLUDE}"
    ${NROS_THREADX_INCLUDES}
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/addons/BSD")
target_compile_definitions(nros_platform_threadx_riscv64 PUBLIC
    ${NROS_THREADX_DEFINES}
    NX_INCLUDE_USER_DEFINE_FILE
    NROS_PLATFORM_BAREMETAL)
target_link_libraries(nros_platform_threadx_riscv64 PUBLIC netxduo threadx_kernel)

set(_nros_rmw_zenoh_threadx_riscv64
    "${_NROS_INSTALL_PREFIX}/lib/libnros_rmw_zenoh_threadx_riscv64.a")
if(NOT EXISTS "${_nros_rmw_zenoh_threadx_riscv64}")
    message(FATAL_ERROR
        "threadx-riscv64-support: ${_nros_rmw_zenoh_threadx_riscv64} not found. "
        "Run `just threadx-riscv64 install` before building C/C++ fixtures.")
endif()
add_library(nros_rmw_zenoh_threadx_riscv64 STATIC IMPORTED)
set_target_properties(nros_rmw_zenoh_threadx_riscv64 PROPERTIES
    IMPORTED_LOCATION "${_nros_rmw_zenoh_threadx_riscv64}")

set(_nros_threadx_riscv64_support_c
    "${CMAKE_CURRENT_BINARY_DIR}/nros_threadx_riscv64_support.c")
file(WRITE "${_nros_threadx_riscv64_support_c}" [=[
extern int nros_rmw_zenoh_register(void);

void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}
]=])
add_library(nros_threadx_riscv64_support STATIC
    "${_nros_threadx_riscv64_support_c}")

nros_threadx_compose_platform(
    COMPONENTS    nros_threadx_riscv64_support
                  nros_rmw_zenoh_threadx_riscv64
                  threadx_kernel
                  nros_platform_threadx_riscv64
                  virtio_net_netx
                  netxduo
    LINK_LIBS    c "${NROS_THREADX_LIBGCC_PATH}"
    LINK_OPTIONS --allow-multiple-definition
                 -L${NROS_THREADX_PICOLIBC_LIB_DIR}
    DEFINES      NROS_PLATFORM_BAREMETAL)

# Linker script + startup glue exports (consumed by per-example
# CMakeLists.txt). Startup source ships under
# share/nano_ros/platform/threadx-riscv64/.
set(THREADX_LINKER_SCRIPT "${THREADX_CONFIG_DIR}/link.lds"
    CACHE FILEPATH "ThreadX RISC-V linker script")
set(THREADX_STARTUP_SOURCE "${_TX_RV64_SHARE}/startup.c")
if(NOT EXISTS "${THREADX_STARTUP_SOURCE}")
    message(FATAL_ERROR
        "threadx-riscv64-support: startup.c not found at ${THREADX_STARTUP_SOURCE}. "
        "Reinstall NanoRos (`just threadx-riscv64 install`).")
endif()
set(THREADX_STARTUP_INCLUDES "${NROS_THREADX_INCLUDES}")

# Backward-compat alias.
function(threadx_riscv64_strip_builtins archive)
    nros_threadx_strip_builtins("${archive}")
endfunction()

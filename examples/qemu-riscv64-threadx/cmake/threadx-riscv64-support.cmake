# threadx-riscv64-support.cmake
#
# CMake support module for ThreadX RISC-V 64-bit (QEMU virt) C/C++
# examples (layer 3). Phase 91.E1a: thin orchestrator on top of
# `nros-threadx.cmake`, which is shipped via the cmake install
# (find_package(NanoRos)).
#
# Provides the `threadx_platform` INTERFACE target plus
# `threadx_riscv64_strip_builtins(<archive>)` for codegen-emitted Rust
# archives that need their soft-float compiler_builtins members
# stripped before linking. Exports THREADX_LINKER_SCRIPT,
# THREADX_STARTUP_SOURCE, and THREADX_STARTUP_INCLUDES so per-example
# CMakeLists.txt files can link their executables.
#
# Required variables (pass via cmake -D or environment):
#   THREADX_DIR        — ThreadX kernel source root
#   NETX_DIR           — NetX Duo source root
#   THREADX_CONFIG_DIR — directory with tx_user.h, nx_user.h, link.lds
#   THREADX_BOARD_DIR  — board crate C dir (entry.s, app_define.c, …)
#   VIRTIO_DRIVER_DIR  — virtio-net-netx driver source dir
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)

include(nros-threadx)

nros_threadx_validate(REQUIRE NETX_DIR THREADX_BOARD_DIR VIRTIO_DRIVER_DIR)

# picolibc must be set up *before* nros_threadx_build_kernel so the
# kernel/netxduo/virtio-net STATIC libs see picolibc headers via the
# global -isystem we install on CMAKE_C_FLAGS / CMAKE_CXX_FLAGS.
nros_threadx_setup_picolibc()

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
    # The board's app_define.c includes nx_api.h. Lump the netx + driver
    # include trees into the kernel target's PRIVATE includes so the
    # board sources find them (they're built as part of threadx_kernel
    # because BOARD_DIR's *.c are globbed in alongside the kernel).
    EXTRA_INCLUDES "${NETX_DIR}/common/inc"
                   "${NETX_DIR}/addons/BSD"
                   "${VIRTIO_DRIVER_DIR}/include"
    EXTRA_DEFINES NX_INCLUDE_USER_DEFINE_FILE NROS_PLATFORM_BAREMETAL)

nros_threadx_build_netstack_netxduo(
    NETX_DIR      "${NETX_DIR}"
    DRIVER_DIR    "${VIRTIO_DRIVER_DIR}"
    EXTRA_DEFINES NROS_PLATFORM_BAREMETAL)

nros_threadx_setup_rust_lld()

# Compose the platform target. picolibc + libgcc are linked manually
# (do NOT use `--specs=picolibc.specs` — it enables TLS errno which
# crashes on bare-metal without OS TLS support). picolibc's libc.a
# defines errno as TLS while ThreadX defines it non-TLS in .sbss; GNU
# `ld` refuses to mix, so we rely on rust-lld at the example level.
# `--allow-multiple-definition` is needed because startup.c overrides
# memset/memcpy/memmove (Rust compiler_builtins versions are buggy on
# RISC-V) and to paper over the same TLS / non-TLS errno mix.
nros_threadx_compose_platform(
    LINK_LIBS    c "${NROS_THREADX_LIBGCC_PATH}"
    LINK_OPTIONS --allow-multiple-definition
                 -L${NROS_THREADX_PICOLIBC_LIB_DIR}
    DEFINES      NROS_PLATFORM_BAREMETAL)

# Linker script + startup glue exports (consumed by per-example
# CMakeLists.txt).
set(THREADX_LINKER_SCRIPT "${THREADX_CONFIG_DIR}/link.lds"
    CACHE FILEPATH "ThreadX RISC-V linker script")
get_filename_component(_TX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(THREADX_STARTUP_SOURCE "${_TX_CMAKE_DIR}/startup.c")
set(THREADX_STARTUP_INCLUDES "${NROS_THREADX_INCLUDES}")

# Backward-compat alias: existing per-example CMakeLists.txt files call
# `threadx_riscv64_strip_builtins(...)`. The layer-2 helper is named
# `nros_threadx_strip_builtins`. Keep both to avoid touching every
# example.
function(threadx_riscv64_strip_builtins archive)
    nros_threadx_strip_builtins("${archive}")
endfunction()

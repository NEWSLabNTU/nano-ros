# threadx-riscv64-support.cmake
#
# Shared CMake support module for ThreadX RISC-V 64-bit (QEMU virt) C/C++ examples.
#
# Provides:
#   threadx_platform         — static library (ThreadX + NetX Duo + virtio-net + startup)
#   THREADX_STARTUP_SOURCE   — startup.c source (provides main → tx_kernel_enter)
#   THREADX_STARTUP_INCLUDES — include dirs for startup.c
#
# Required variables (pass via cmake -D or environment):
#   THREADX_DIR           — ThreadX kernel source root
#   NETX_DIR              — NetX Duo source root
#   THREADX_CONFIG_DIR    — Directory containing tx_user.h, nx_user.h
#   THREADX_BOARD_DIR     — Board crate C directory (entry.s, app_define.c, etc.)
#   VIRTIO_DRIVER_DIR     — virtio-net-netx driver source directory

# ---- Validate required variables ----
foreach(_var THREADX_DIR NETX_DIR THREADX_CONFIG_DIR THREADX_BOARD_DIR VIRTIO_DRIVER_DIR)
    if(NOT DEFINED ${_var})
        if(DEFINED ENV{${_var}})
            set(${_var} "$ENV{${_var}}")
        else()
            message(FATAL_ERROR "${_var} not set. Pass -D${_var}=<path> or export ${_var}.")
        endif()
    endif()
endforeach()

set(_TX_PORT_DIR "${THREADX_DIR}/ports/risc-v64/gnu")
set(_TX_QEMU_VIRT_DIR "${_TX_PORT_DIR}/example_build/qemu_virt")

# ---- Shared include directories ----
set(_TX_INCLUDES
    "${THREADX_CONFIG_DIR}"
    "${THREADX_DIR}/common/inc"
    "${_TX_PORT_DIR}/inc"
    "${_TX_QEMU_VIRT_DIR}"
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/addons/BSD"
    "${VIRTIO_DRIVER_DIR}/include"
)

# ---- Detect picolibc sysroot (provides string.h, stdint.h, etc.) ----
# Try --specs=picolibc.specs first, then well-known system paths.
execute_process(
    COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d --specs=picolibc.specs -print-sysroot
    OUTPUT_VARIABLE _PICOLIBC_SYSROOT
    OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET
)
if(NOT _PICOLIBC_SYSROOT OR NOT EXISTS "${_PICOLIBC_SYSROOT}/include")
    # Debian/Ubuntu installs picolibc here
    set(_PICOLIBC_SYSROOT "/usr/lib/picolibc/riscv64-unknown-elf")
endif()
if(EXISTS "${_PICOLIBC_SYSROOT}/include")
    message(STATUS "picolibc sysroot: ${_PICOLIBC_SYSROOT}")
    list(APPEND _TX_INCLUDES "${_PICOLIBC_SYSROOT}/include")
    # Also set globally so codegen-generated C/C++ sources can find stdint.h etc.
    # NROS_PLATFORM_BAREMETAL prevents nros headers from pulling in POSIX-only code.
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -isystem ${_PICOLIBC_SYSROOT}/include -DNROS_PLATFORM_BAREMETAL" PARENT_SCOPE)
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -isystem ${_PICOLIBC_SYSROOT}/include -DNROS_PLATFORM_BAREMETAL")
    # C++ compat headers: provide <cstdio>, <cstdint> etc. wrapping picolibc C headers
    get_filename_component(_CXX_COMPAT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
    set(_CXX_COMPAT_DIR "${_CXX_COMPAT_DIR}/cxx-compat")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -isystem ${_PICOLIBC_SYSROOT}/include -isystem ${_CXX_COMPAT_DIR} -DNROS_PLATFORM_BAREMETAL" PARENT_SCOPE)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -isystem ${_PICOLIBC_SYSROOT}/include -isystem ${_CXX_COMPAT_DIR} -DNROS_PLATFORM_BAREMETAL")
else()
    message(WARNING "picolibc sysroot not found — C standard library headers may be missing.\n"
        "Install: sudo apt install picolibc-riscv64-unknown-elf")
endif()

# ---- Common compile definitions ----
# NROS_PLATFORM_BAREMETAL avoids pulling in POSIX-specific headers (time.h, etc.)
# from the nros-c platform.h header. ThreadX has its own timer implementation.
set(_TX_DEFS TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE NROS_PLATFORM_BAREMETAL)

# ---- ThreadX kernel library ----
file(GLOB _tx_kernel_srcs "${THREADX_DIR}/common/src/*.c")
file(GLOB _tx_port_c_srcs "${_TX_PORT_DIR}/src/*.c")

# Port assembly — exclude files overridden by board crate
file(GLOB _tx_port_asm_all "${_TX_PORT_DIR}/src/*.S")
set(_tx_excluded_asm
    tx_initialize_low_level.S
    tx_thread_schedule.S
    tx_thread_context_save.S
    tx_thread_context_restore.S
    tx_thread_stack_build.S
    tx_thread_system_return.S
)
set(_tx_port_asm "")
foreach(_f ${_tx_port_asm_all})
    get_filename_component(_name ${_f} NAME)
    list(FIND _tx_excluded_asm "${_name}" _idx)
    if(_idx EQUAL -1)
        list(APPEND _tx_port_asm ${_f})
    endif()
endforeach()

# Board-specific assembly overrides
file(GLOB _board_asm "${THREADX_BOARD_DIR}/*.S" "${THREADX_BOARD_DIR}/*.s")
file(GLOB _board_c "${THREADX_BOARD_DIR}/*.c")

# QEMU virt board C files (exclude trap.c and hwtimer.c — board crate provides its own)
file(GLOB _qemu_virt_c_all "${_TX_QEMU_VIRT_DIR}/*.c")
set(_qemu_virt_excluded trap.c hwtimer.c demo_threadx.c)
set(_qemu_virt_srcs "")
foreach(_f ${_qemu_virt_c_all})
    get_filename_component(_name ${_f} NAME)
    list(FIND _qemu_virt_excluded "${_name}" _idx)
    if(_idx EQUAL -1)
        list(APPEND _qemu_virt_srcs ${_f})
    endif()
endforeach()
# QEMU virt assembly (tx_initialize_low_level.S — entry.s from board crate instead)
list(APPEND _qemu_virt_srcs "${_TX_QEMU_VIRT_DIR}/tx_initialize_low_level.S")

add_library(threadx_kernel STATIC
    ${_tx_kernel_srcs} ${_tx_port_c_srcs} ${_tx_port_asm}
    ${_board_asm} ${_board_c}
    ${_qemu_virt_srcs}
)
target_include_directories(threadx_kernel PRIVATE ${_TX_INCLUDES})
target_compile_definitions(threadx_kernel PRIVATE ${_TX_DEFS})
target_compile_options(threadx_kernel PRIVATE
    -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(threadx_kernel PROPERTIES C_STANDARD 11)

# ---- NetX Duo library ----
file(GLOB _netx_srcs "${NETX_DIR}/common/src/*.c")
add_library(netxduo STATIC ${_netx_srcs} "${NETX_DIR}/addons/BSD/nxd_bsd.c")
target_include_directories(netxduo PRIVATE ${_TX_INCLUDES})
target_compile_definitions(netxduo PRIVATE ${_TX_DEFS})
target_compile_options(netxduo PRIVATE -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(netxduo PROPERTIES C_STANDARD 11)

# ---- Virtio-net NetX Duo driver ----
file(GLOB _virtio_srcs "${VIRTIO_DRIVER_DIR}/src/*.c")
add_library(virtio_net_netx STATIC ${_virtio_srcs})
target_include_directories(virtio_net_netx PRIVATE ${_TX_INCLUDES})
target_compile_definitions(virtio_net_netx PRIVATE ${_TX_DEFS})
target_compile_options(virtio_net_netx PRIVATE -Wno-unused-parameter -Wno-sign-compare)
set_target_properties(virtio_net_netx PROPERTIES C_STANDARD 11)

# ---- Find picolibc and libgcc for linking ----
set(_PICOLIBC_LIB_DIR "${_PICOLIBC_SYSROOT}/lib/rv64imafdc/lp64d")
if(NOT EXISTS "${_PICOLIBC_LIB_DIR}/libc.a")
    # Fallback: try to get it from gcc
    execute_process(
        COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d --specs=picolibc.specs -print-file-name=libc.a
        OUTPUT_VARIABLE _picolibc_path OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
    if(_picolibc_path)
        get_filename_component(_PICOLIBC_LIB_DIR "${_picolibc_path}" DIRECTORY)
    endif()
endif()
execute_process(
    COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d -print-libgcc-file-name
    OUTPUT_VARIABLE _LIBGCC_PATH OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)

# ---- Combined platform target ----
add_library(threadx_platform INTERFACE)
target_link_libraries(threadx_platform INTERFACE
    virtio_net_netx netxduo threadx_kernel)
target_include_directories(threadx_platform INTERFACE ${_TX_INCLUDES})
target_compile_definitions(threadx_platform INTERFACE NROS_PLATFORM_BAREMETAL)
# Link picolibc + libgcc manually (do NOT use --specs=picolibc.specs — it
# enables TLS errno which crashes on bare-metal without OS TLS support).
#
# picolibc's libc.a defines errno as TLS, but ThreadX defines it as non-TLS
# (.sbss). GNU ld refuses to mix TLS/non-TLS references. Use rust-lld
# (which handles this like the Rust build does) via -fuse-ld=lld.
execute_process(
    COMMAND rustc --print sysroot
    OUTPUT_VARIABLE _RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
find_program(_RUST_LLD rust-lld
    PATHS "${_RUST_SYSROOT}/lib/rustlib/x86_64-unknown-linux-gnu/bin"
    NO_DEFAULT_PATH)
# Link picolibc + libgcc. The toolchain file (riscv64-threadx.cmake) sets up
# rust-lld via -B wrapper, which handles the TLS errno mismatch between
# picolibc (TLS) and ThreadX (non-TLS).
target_link_options(threadx_platform INTERFACE
    -L${_PICOLIBC_LIB_DIR}
)
target_link_libraries(threadx_platform INTERFACE c "${_LIBGCC_PATH}")

# ---- Linker script ----
set(THREADX_LINKER_SCRIPT "${THREADX_CONFIG_DIR}/link.lds" CACHE FILEPATH "ThreadX RISC-V linker script")

# ---- Startup source ----
get_filename_component(_TX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(THREADX_STARTUP_SOURCE "${_TX_CMAKE_DIR}/startup.c")
set(THREADX_STARTUP_INCLUDES ${_TX_INCLUDES})

# ---- Helper: strip soft-float compiler_builtins from any Rust archive ----
# Usage: threadx_riscv64_strip_builtins(<archive_path>)
# Call this on codegen-generated FFI libraries before linking.
execute_process(COMMAND rustc --print sysroot
    OUTPUT_VARIABLE _TX_RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
find_program(_TX_LLVM_AR llvm-ar
    PATHS "${_TX_RUST_SYSROOT}/lib/rustlib/x86_64-unknown-linux-gnu/bin" NO_DEFAULT_PATH)
set(_TX_STRIP_SCRIPT "${_TX_CMAKE_DIR}/../../../cmake/strip-compiler-builtins.sh")

function(threadx_riscv64_strip_builtins archive)
    if(_TX_LLVM_AR AND EXISTS "${_TX_STRIP_SCRIPT}")
        add_custom_command(OUTPUT "${archive}.stripped"
            COMMAND bash "${_TX_STRIP_SCRIPT}" "${_TX_LLVM_AR}" "${archive}"
            COMMAND ${CMAKE_COMMAND} -E touch "${archive}.stripped"
            DEPENDS "${archive}"
            COMMENT "Stripping soft-float builtins from ${archive}"
        )
    endif()
endfunction()

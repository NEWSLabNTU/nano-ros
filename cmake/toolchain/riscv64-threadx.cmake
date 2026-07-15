# cmake/toolchain/riscv64-threadx.cmake
#
# CMake toolchain file for ThreadX on RISC-V 64-bit (QEMU virt).
#
# Selects the riscv64-unknown-elf cross-compiler and sets the Rust target
# triple so that Corrosion compiles nros-c / nros-cpp for riscv64gc.
#
# Usage:
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/riscv64-threadx.cmake \
#         -DNANO_ROS_RMW=zenoh \
#         -DNANO_ROS_PLATFORM=threadx_riscv64 \
#         -DNANO_ROS_BUILD_CODEGEN=OFF
#   cmake --build build
#   cmake --install build --prefix /path/to/prefix

set(CMAKE_SYSTEM_NAME       Generic)
set(CMAKE_SYSTEM_PROCESSOR  riscv64)

set(CMAKE_C_COMPILER    riscv64-unknown-elf-gcc)
set(CMAKE_CXX_COMPILER  riscv64-unknown-elf-g++)
set(CMAKE_ASM_COMPILER  riscv64-unknown-elf-gcc)
set(CMAKE_AR            riscv64-unknown-elf-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        riscv64-unknown-elf-ranlib CACHE FILEPATH "Ranlib")

set(CMAKE_C_FLAGS_INIT   "-march=rv64gc -mabi=lp64d -mcmodel=medany -ffunction-sections -fdata-sections -fno-builtin")
set(CMAKE_CXX_FLAGS_INIT "-march=rv64gc -mabi=lp64d -mcmodel=medany -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14 -ffreestanding")
set(CMAKE_ASM_FLAGS_INIT "-march=rv64gc -mabi=lp64d -mcmodel=medany")

# Phase 155.E — picolibc include dir for every target compiled
# under this toolchain. gcc's `<stdint.h>` does `#include_next
# <stdint.h>` expecting libc to provide the real one — bare-
# metal RISC-V uses picolibc. Without this on EVERY target's
# include path, codegen-output `.c` files (`std_msgs__nano_ros_c`
# etc.) that don't go through `nros_threadx_setup_picolibc`
# fail at `fatal error: stdint.h: No such file or directory`.
execute_process(
    COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d
            --specs=picolibc.specs -print-sysroot
    OUTPUT_VARIABLE _RISCV_THREADX_PICOLIBC_SYSROOT
    OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET)
if(NOT _RISCV_THREADX_PICOLIBC_SYSROOT
        OR NOT EXISTS "${_RISCV_THREADX_PICOLIBC_SYSROOT}/include")
    # Debian / Ubuntu picolibc-riscv64-unknown-elf install path
    set(_RISCV_THREADX_PICOLIBC_SYSROOT
        "/usr/lib/picolibc/riscv64-unknown-elf")
endif()
if(EXISTS "${_RISCV_THREADX_PICOLIBC_SYSROOT}/include")
    set(CMAKE_C_FLAGS_INIT
        "${CMAKE_C_FLAGS_INIT} -isystem ${_RISCV_THREADX_PICOLIBC_SYSROOT}/include")
    set(CMAKE_CXX_FLAGS_INIT
        "${CMAKE_CXX_FLAGS_INIT} -isystem ${_RISCV_THREADX_PICOLIBC_SYSROOT}/include")
endif()

# Phase 155.E — C++ shim headers (`cstdio`, `cstdint`, etc.)
# that wrap picolibc's C headers in the `std::` namespace.
# The Debian `picolibc-riscv64-unknown-elf` package is C-only;
# libstdc++ for riscv64-unknown-elf doesn't ship in apt. The
# board crate's `cxx-compat/` dir provides the freestanding
# minimum every nano-ros C++ example needs (Phase 89.13 +
# follow-ups). Apply globally so codegen-output `.cpp` files +
# `nros-cpp` headers pulled by examples both see it.
get_filename_component(_riscv_threadx_cxx_compat
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat"
    ABSOLUTE)
if(EXISTS "${_riscv_threadx_cxx_compat}/cstdio")
    set(CMAKE_CXX_FLAGS_INIT
        "${CMAKE_CXX_FLAGS_INIT} -isystem ${_riscv_threadx_cxx_compat}")
endif()

# Issue #195 — locate a rv64gc/lp64d `libstdc++.a` for the Cyclone DDS RMW
# (NanoRosRmwDispatch adds `stdc++` to the cyclone link set: the wrapper is
# C++ — operator new/delete, guards). The Debian
# `gcc-riscv64-unknown-elf`/picolibc packages ship NO libstdc++, so resolve:
#   1. the active compiler's own multilib (xpack-style toolchains have it),
#   2. else the nros SDK `riscv-none-elf-gcc` multilib
#      (rv64imafdc_zicsr = rv64gc, lp64d — provisioned by `nros setup`).
# Zenoh-only builds never reference `-lstdc++`, so a miss here changes
# nothing for them; the cyclone link fails loudly either way.
execute_process(
    COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d
            --print-file-name=libstdc++.a
    OUTPUT_VARIABLE _riscv_stdcxx OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
if(NOT _riscv_stdcxx MATCHES "^/" OR NOT EXISTS "${_riscv_stdcxx}")
    file(GLOB _riscv_sdk_stdcxx
        "$ENV{HOME}/.nros/sdk/riscv-none-elf-gcc/*/riscv-none-elf/lib/rv64imafdc_zicsr/lp64d/libstdc++.a")
    if(_riscv_sdk_stdcxx)
        list(GET _riscv_sdk_stdcxx -1 _riscv_stdcxx)
    else()
        set(_riscv_stdcxx "")
    endif()
endif()
if(_riscv_stdcxx)
    get_filename_component(_riscv_stdcxx_dir "${_riscv_stdcxx}" DIRECTORY)
    add_link_options("-L${_riscv_stdcxx_dir}")
    # The SDK libstdc++ is newlib-built: its internals reference newlib reent
    # objects whose syscalls (`_sbrk`/`_read`/`_kill`/…) nothing bare-metal
    # provides. `libnosys.a` (same multilib dir) stubs them; appended at the
    # END of the link line (STANDARD_LIBRARIES) so it only satisfies leftovers
    # — the image's real malloc/IO (picolibc + the ThreadX platform) resolve
    # first. Runtime-correct for Cyclone: transient samples go through
    # ddsrt_{malloc,free} on the ThreadX byte pool, never these stubs.
    if(EXISTS "${_riscv_stdcxx_dir}/libnosys.a")
        set(CMAKE_C_STANDARD_LIBRARIES "-lnosys" CACHE STRING "" FORCE)
        set(CMAKE_CXX_STANDARD_LIBRARIES "-lnosys" CACHE STRING "" FORCE)
    endif()
endif()

# Rust target triple
set(Rust_CARGO_TARGET "riscv64gc-unknown-none-elf" CACHE STRING "Rust target triple" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)

# Fix compiler_builtins float ABI: force +d feature so bswapsi2 etc.
# use hard-float ABI matching lp64d. This applies to ALL Rust builds
# in this CMake session (including codegen FFI crates).
# See: https://github.com/rust-lang/rust/issues/83229
set(ENV{RUSTFLAGS} "-Ctarget-feature=+d")

# Use rust-lld as the linker instead of GNU ld.
# picolibc's libc.a has TLS errno which GNU ld refuses to link with
# ThreadX's non-TLS errno. LLD handles this correctly (like Rust does).
# GCC 10.x doesn't support -fuse-ld=lld for cross targets, so we override
# the entire link rule via CMAKE_C_LINK_EXECUTABLE.
execute_process(
    COMMAND rustc --print sysroot
    OUTPUT_VARIABLE _RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
find_program(_RUST_LLD rust-lld
    PATHS "${_RUST_SYSROOT}/lib/rustlib/x86_64-unknown-linux-gnu/bin"
    NO_DEFAULT_PATH)
if(_RUST_LLD)
    # Pass tool paths to the wrapper via environment variables. Earlier
    # revisions of this toolchain materialised symlinks at
    # `${CMAKE_CURRENT_LIST_DIR}/_real_lld` / `_llvm_ar` so the wrapper
    # could resolve its sibling tools by `dirname "$0"`. Two problems
    # with that: (1) it raced when two cmake configures ran in parallel
    # against the same toolchain (nextest does this routinely) — both
    # tried to create the same in-source symlink and the loser aborted
    # with `file failed to create symbolic link: File exists`; (2) it
    # wrote into the source tree as a side effect of configure. Env
    # vars sidestep both: each link invocation carries its own
    # `NROS_RUST_LLD` / `NROS_LLVM_AR` and the toolchain dir stays
    # read-only.
    get_filename_component(_lld_dir "${_RUST_LLD}" DIRECTORY)
    find_program(_LLVM_AR_TC llvm-ar PATHS "${_lld_dir}" NO_DEFAULT_PATH)
    if(NOT _LLVM_AR_TC)
        message(FATAL_ERROR
            "llvm-ar not found alongside rust-lld at ${_lld_dir} — needed by "
            "riscv64-lld-wrapper.sh to strip soft-float compiler_builtins.")
    endif()

    set(_lld_wrapper "${CMAKE_CURRENT_LIST_DIR}/riscv64-lld-wrapper.sh")
    set(CMAKE_LINKER "${_lld_wrapper}" CACHE FILEPATH "Linker" FORCE)

    # Override link rules. The wrapper strips soft-float compiler_builtins
    # from all .a archives, then delegates to rust-lld. `cmake -E env`
    # injects the tool paths so the wrapper doesn't need siblings.
    set(_lld_env "${CMAKE_COMMAND} -E env NROS_RUST_LLD=${_RUST_LLD} NROS_LLVM_AR=${_LLVM_AR_TC}")
    set(CMAKE_C_LINK_EXECUTABLE
        "${_lld_env} bash ${_lld_wrapper} -flavor gnu <CMAKE_C_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>"
        CACHE STRING "" FORCE)
    set(CMAKE_CXX_LINK_EXECUTABLE
        "${_lld_env} bash ${_lld_wrapper} -flavor gnu <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>"
        CACHE STRING "" FORCE)
endif()

# nros-threadx.cmake
#
# Per-RTOS cmake module for ThreadX. Built on top of
# nros-rtos-helpers.cmake. Encodes ThreadX-specific knowledge once so
# layer-3 per-platform support files (threadx-linux, threadx-riscv64,
# future ThreadX ports) shrink to ~15-line orchestrators.
#
# Public functions:
#
#   nros_threadx_validate(REQUIRE <vars…>)
#       Validate the listed cmake variables (env-or-fatal-error) and
#       compute the ThreadX include set into the parent-scope
#       NROS_THREADX_INCLUDES variable. Always requires THREADX_DIR and
#       THREADX_CONFIG_DIR plus whatever the caller passes in REQUIRE.
#
#   nros_threadx_build_kernel(PORT <subdir>
#                             [BOARD_DIR <dir>]
#                             [BOARD_OVERRIDES <asm files…>]
#                             [QEMU_VIRT_DIR <dir>]
#                             [QEMU_VIRT_EXCLUDE <c files…>])
#       Build the threadx_kernel STATIC library. PORT is the suffix
#       under "${THREADX_DIR}/ports/" (e.g. "linux/gnu",
#       "risc-v64/gnu"). For ports with board-supplied
#       reset/scheduler/context-switch assembly, pass BOARD_DIR
#       (extra C + asm sources) and BOARD_OVERRIDES (asm files in the
#       port to *exclude* in favour of the board's). QEMU_VIRT_DIR /
#       QEMU_VIRT_EXCLUDE handle the "qemu_virt example_build" tree
#       used by the RISC-V port.
#
#   nros_threadx_build_netstack_nsos(SHIM_DIR <path>)
#       Linux-style: build nsos_netx STATIC from a single .c file in
#       <SHIM_DIR>/src plus its include dir. Used when the ThreadX
#       port runs on top of a host POSIX network stack.
#
#   nros_threadx_build_netstack_netxduo(NETX_DIR <path>
#                                       [DRIVER_DIR <path>]
#                                       [EXTRA_DEFINES <defs…>])
#       Bare-metal-style: build netxduo STATIC from
#       "${NETX_DIR}/common/src/*.c" + the BSD addon, plus an
#       optional virtio-style driver lib from <DRIVER_DIR>/src.
#
#   nros_threadx_build_glue(SOURCES <files…> [DEFINES <defs…>])
#       Build threadx_glue STATIC from caller-supplied app_define.c /
#       board init source. Linked into the platform target so the
#       symbols (`tx_application_define`, etc.) reach the final ELF.
#
#   nros_threadx_setup_picolibc()
#       Locate picolibc's sysroot via `--specs=picolibc.specs
#       -print-sysroot`, fall back to Debian's /usr/lib/picolibc/...
#       layout. Adds `-isystem` flags to CMAKE_C_FLAGS and
#       CMAKE_CXX_FLAGS plus `-DNROS_PLATFORM_BAREMETAL`. Sets the
#       parent-scope NROS_THREADX_PICOLIBC_LIB_DIR.
#
#   nros_threadx_setup_rust_lld()
#       Locate rust-lld via `rustc --print sysroot`. Sets the
#       parent-scope NROS_THREADX_LLD_PATH for callers that need to
#       set CMAKE_LINKER or pass -fuse-ld=lld.
#
#   nros_threadx_strip_builtins(<archive>)
#       Custom-command helper for RISC-V soft-float / hard-float
#       mismatches: strips the soft-float compiler_builtins members
#       from a Rust-emitted archive. Outputs <archive>.stripped.
#
#   nros_threadx_compose_platform([COMPONENTS <libs…>]
#                                 [LINK_LIBS <libs…>]
#                                 [LINK_OPTIONS <opts…>])
#       Compose threadx_platform INTERFACE. By default links
#       threadx_glue, the active netstack, threadx_kernel; pass
#       COMPONENTS to override. LINK_LIBS adds system libraries
#       (e.g. pthread on Linux, picolibc + libgcc on RV64).
#       LINK_OPTIONS forwards INTERFACE linker flags.
#
# Variables set by this module (parent scope of caller after
# nros_threadx_validate / setup_*):
#
#   NROS_THREADX_INCLUDES         — include set used by all kernel/netstack libs
#   NROS_THREADX_PORT_DIR         — "${THREADX_DIR}/ports/${PORT}"
#   NROS_THREADX_DEFINES          — TX_INCLUDE_USER_DEFINE_FILE etc.
#   NROS_THREADX_PICOLIBC_LIB_DIR — picolibc archive directory (after setup_picolibc)
#   NROS_THREADX_LLD_PATH         — rust-lld path (after setup_rust_lld)

if(DEFINED _NROS_THREADX_INCLUDED)
    return()
endif()
set(_NROS_THREADX_INCLUDED TRUE)

include("${CMAKE_CURRENT_LIST_DIR}/nros-rtos-helpers.cmake")

# ----------------------------------------------------------------------
# nros_threadx_validate
# ----------------------------------------------------------------------
function(nros_threadx_validate)
    cmake_parse_arguments(_NTV "" "" "REQUIRE" ${ARGN})
    nros_validate_vars(THREADX_DIR THREADX_CONFIG_DIR ${_NTV_REQUIRE})

    # Re-export resolved paths so the caller's parent scope sees them
    # (nros_validate_vars sets them in *its* parent scope, which is
    # this function's frame — propagate one more level up).
    set(THREADX_DIR        "${THREADX_DIR}"        PARENT_SCOPE)
    set(THREADX_CONFIG_DIR "${THREADX_CONFIG_DIR}" PARENT_SCOPE)
    foreach(_v ${_NTV_REQUIRE})
        set(${_v} "${${_v}}" PARENT_SCOPE)
    endforeach()

    set(NROS_THREADX_DEFINES TX_INCLUDE_USER_DEFINE_FILE PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_build_kernel
# ----------------------------------------------------------------------
function(nros_threadx_build_kernel)
    cmake_parse_arguments(_NTBK
        ""
        "PORT;BOARD_DIR;QEMU_VIRT_DIR"
        "BOARD_OVERRIDES;QEMU_VIRT_EXCLUDE;EXTRA_DEFINES;EXTRA_INCLUDES"
        ${ARGN})

    if(NOT _NTBK_PORT)
        message(FATAL_ERROR "nros_threadx_build_kernel: PORT is required.")
    endif()

    set(_port_dir "${THREADX_DIR}/ports/${_NTBK_PORT}")
    set(_includes
        "${THREADX_CONFIG_DIR}"
        "${THREADX_DIR}/common/inc"
        "${_port_dir}/inc")
    if(_NTBK_QEMU_VIRT_DIR)
        list(APPEND _includes "${_NTBK_QEMU_VIRT_DIR}")
    endif()
    if(_NTBK_EXTRA_INCLUDES)
        list(APPEND _includes ${_NTBK_EXTRA_INCLUDES})
    endif()

    file(GLOB _kernel_srcs "${THREADX_DIR}/common/src/*.c")
    file(GLOB _port_c_srcs "${_port_dir}/src/*.c")

    # Port assembly with optional board overrides
    file(GLOB _port_asm_all "${_port_dir}/src/*.S")
    set(_port_asm "")
    foreach(_f ${_port_asm_all})
        get_filename_component(_name "${_f}" NAME)
        list(FIND _NTBK_BOARD_OVERRIDES "${_name}" _idx)
        if(_idx EQUAL -1)
            list(APPEND _port_asm "${_f}")
        endif()
    endforeach()

    # Optional board-supplied glue (assembly + C)
    set(_board_srcs "")
    if(_NTBK_BOARD_DIR)
        file(GLOB _board_asm "${_NTBK_BOARD_DIR}/*.S" "${_NTBK_BOARD_DIR}/*.s")
        file(GLOB _board_c   "${_NTBK_BOARD_DIR}/*.c")
        list(APPEND _board_srcs ${_board_asm} ${_board_c})
    endif()

    # Optional QEMU-virt example_build C/asm tree (RISC-V port)
    set(_qemu_virt_srcs "")
    if(_NTBK_QEMU_VIRT_DIR)
        file(GLOB _qv_c_all "${_NTBK_QEMU_VIRT_DIR}/*.c")
        foreach(_f ${_qv_c_all})
            get_filename_component(_name "${_f}" NAME)
            list(FIND _NTBK_QEMU_VIRT_EXCLUDE "${_name}" _idx)
            if(_idx EQUAL -1)
                list(APPEND _qemu_virt_srcs "${_f}")
            endif()
        endforeach()
        # QEMU-virt's tx_initialize_low_level.S is the platform's actual
        # low-level init — listed in BOARD_OVERRIDES above to *exclude*
        # the generic port version, then re-added here from the
        # qemu_virt example_build subdir.
        list(APPEND _qemu_virt_srcs
             "${_NTBK_QEMU_VIRT_DIR}/tx_initialize_low_level.S")
    endif()

    nros_build_rtos_static_lib(threadx_kernel
        SOURCES ${_kernel_srcs} ${_port_c_srcs} ${_port_asm}
                ${_board_srcs} ${_qemu_virt_srcs}
        INCLUDES ${_includes}
        DEFINES  ${NROS_THREADX_DEFINES} ${_NTBK_EXTRA_DEFINES})

    set(NROS_THREADX_INCLUDES "${_includes}" PARENT_SCOPE)
    set(NROS_THREADX_PORT_DIR "${_port_dir}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_build_netstack_nsos (Linux / POSIX shim)
# ----------------------------------------------------------------------
function(nros_threadx_build_netstack_nsos)
    cmake_parse_arguments(_NTNN "" "SHIM_DIR" "" ${ARGN})
    if(NOT _NTNN_SHIM_DIR)
        message(FATAL_ERROR
            "nros_threadx_build_netstack_nsos: SHIM_DIR is required.")
    endif()

    nros_build_rtos_static_lib(nsos_netx
        SOURCES "${_NTNN_SHIM_DIR}/src/nsos_netx.c")
    target_include_directories(nsos_netx PUBLIC "${_NTNN_SHIM_DIR}/include")
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_build_netstack_netxduo (real NetX Duo + driver)
# ----------------------------------------------------------------------
function(nros_threadx_build_netstack_netxduo)
    cmake_parse_arguments(_NTND
        ""
        "NETX_DIR;DRIVER_DIR"
        "EXTRA_DEFINES"
        ${ARGN})
    if(NOT _NTND_NETX_DIR)
        message(FATAL_ERROR
            "nros_threadx_build_netstack_netxduo: NETX_DIR is required.")
    endif()

    file(GLOB _netx_srcs "${_NTND_NETX_DIR}/common/src/*.c")
    set(_netx_includes
        ${NROS_THREADX_INCLUDES}
        "${_NTND_NETX_DIR}/common/inc"
        "${_NTND_NETX_DIR}/addons/BSD")

    nros_build_rtos_static_lib(netxduo
        SOURCES ${_netx_srcs} "${_NTND_NETX_DIR}/addons/BSD/nxd_bsd.c"
        INCLUDES ${_netx_includes}
        DEFINES  ${NROS_THREADX_DEFINES} NX_INCLUDE_USER_DEFINE_FILE
                 ${_NTND_EXTRA_DEFINES})

    if(_NTND_DRIVER_DIR)
        file(GLOB _drv_srcs "${_NTND_DRIVER_DIR}/src/*.c")
        nros_build_rtos_static_lib(virtio_net_netx
            SOURCES ${_drv_srcs}
            INCLUDES ${_netx_includes} "${_NTND_DRIVER_DIR}/include"
            DEFINES  ${NROS_THREADX_DEFINES} NX_INCLUDE_USER_DEFINE_FILE
                     ${_NTND_EXTRA_DEFINES})
    endif()
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_build_glue
# ----------------------------------------------------------------------
function(nros_threadx_build_glue)
    cmake_parse_arguments(_NTBG "" "" "SOURCES;DEFINES" ${ARGN})
    if(NOT _NTBG_SOURCES)
        message(FATAL_ERROR "nros_threadx_build_glue: SOURCES is required.")
    endif()

    nros_build_rtos_static_lib(threadx_glue
        SOURCES ${_NTBG_SOURCES}
        INCLUDES ${NROS_THREADX_INCLUDES}
        DEFINES  ${NROS_THREADX_DEFINES} ${_NTBG_DEFINES})
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_setup_picolibc (RISC-V / bare-metal hosts)
# ----------------------------------------------------------------------
function(nros_threadx_setup_picolibc)
    execute_process(
        COMMAND ${CMAKE_C_COMPILER} -march=rv64gc -mabi=lp64d
                --specs=picolibc.specs -print-sysroot
        OUTPUT_VARIABLE _sysroot
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    if(NOT _sysroot OR NOT EXISTS "${_sysroot}/include")
        # Debian / Ubuntu picolibc-riscv64-unknown-elf install path
        set(_sysroot "/usr/lib/picolibc/riscv64-unknown-elf")
    endif()
    if(NOT EXISTS "${_sysroot}/include")
        message(WARNING
            "picolibc sysroot not found — C standard library headers may be missing.\n"
            "Install: sudo apt install picolibc-riscv64-unknown-elf")
        return()
    endif()
    message(STATUS "picolibc sysroot: ${_sysroot}")

    # cxx-compat shim dir lives next to the per-platform support file.
    # The caller's CMAKE_CURRENT_LIST_DIR is the example's cmake dir,
    # so we resolve it from the variable that's already in scope when
    # this function runs.
    get_filename_component(_caller_dir "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
    set(_cxx_compat "${_caller_dir}/cxx-compat")

    set(CMAKE_C_FLAGS
        "${CMAKE_C_FLAGS} -isystem ${_sysroot}/include -DNROS_PLATFORM_BAREMETAL"
        PARENT_SCOPE)
    set(CMAKE_CXX_FLAGS
        "${CMAKE_CXX_FLAGS} -isystem ${_sysroot}/include -isystem ${_cxx_compat} -DNROS_PLATFORM_BAREMETAL"
        PARENT_SCOPE)

    set(_lib_dir "${_sysroot}/lib/rv64imafdc/lp64d")
    if(NOT EXISTS "${_lib_dir}/libc.a")
        execute_process(
            COMMAND ${CMAKE_C_COMPILER} -march=rv64gc -mabi=lp64d
                    --specs=picolibc.specs -print-file-name=libc.a
            OUTPUT_VARIABLE _libc_path
            OUTPUT_STRIP_TRAILING_WHITESPACE
            ERROR_QUIET)
        if(_libc_path)
            get_filename_component(_lib_dir "${_libc_path}" DIRECTORY)
        endif()
    endif()
    set(NROS_THREADX_PICOLIBC_LIB_DIR "${_lib_dir}" PARENT_SCOPE)

    execute_process(
        COMMAND ${CMAKE_C_COMPILER} -march=rv64gc -mabi=lp64d
                -print-libgcc-file-name
        OUTPUT_VARIABLE _libgcc
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    set(NROS_THREADX_LIBGCC_PATH "${_libgcc}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_setup_rust_lld (RISC-V picolibc TLS-vs-non-TLS errno mix)
# ----------------------------------------------------------------------
function(nros_threadx_setup_rust_lld)
    execute_process(
        COMMAND rustc --print sysroot
        OUTPUT_VARIABLE _rust_sysroot
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    find_program(_rust_lld rust-lld
        PATHS "${_rust_sysroot}/lib/rustlib/x86_64-unknown-linux-gnu/bin"
        NO_DEFAULT_PATH)
    set(NROS_THREADX_LLD_PATH "${_rust_lld}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_strip_builtins
# ----------------------------------------------------------------------
function(nros_threadx_strip_builtins archive)
    if(NOT DEFINED NROS_THREADX_STRIP_SCRIPT)
        execute_process(
            COMMAND rustc --print sysroot
            OUTPUT_VARIABLE _rust_sysroot
            OUTPUT_STRIP_TRAILING_WHITESPACE
            ERROR_QUIET)
        find_program(_llvm_ar llvm-ar
            PATHS "${_rust_sysroot}/lib/rustlib/x86_64-unknown-linux-gnu/bin"
            NO_DEFAULT_PATH)
        set(NROS_THREADX_LLVM_AR "${_llvm_ar}" CACHE FILEPATH "" FORCE)
        # The strip script ships next to this module in the install
        # tree; resolve it from CMAKE_CURRENT_LIST_DIR (where this
        # cmake file sits at include() time).
        set(NROS_THREADX_STRIP_SCRIPT
            "${CMAKE_CURRENT_LIST_DIR}/strip-compiler-builtins.sh"
            CACHE FILEPATH "" FORCE)
    endif()

    if(NROS_THREADX_LLVM_AR AND EXISTS "${NROS_THREADX_STRIP_SCRIPT}")
        add_custom_command(OUTPUT "${archive}.stripped"
            COMMAND bash "${NROS_THREADX_STRIP_SCRIPT}"
                    "${NROS_THREADX_LLVM_AR}" "${archive}"
            COMMAND ${CMAKE_COMMAND} -E touch "${archive}.stripped"
            DEPENDS "${archive}"
            COMMENT "Stripping soft-float builtins from ${archive}")
    endif()
endfunction()

# ----------------------------------------------------------------------
# nros_threadx_compose_platform
# ----------------------------------------------------------------------
function(nros_threadx_compose_platform)
    cmake_parse_arguments(_NTCP
        ""
        ""
        "COMPONENTS;LINK_LIBS;LINK_OPTIONS;DEFINES"
        ${ARGN})

    # Default component list: glue (if it was built), netstack
    # (auto-detected by target existence), kernel.
    if(NOT _NTCP_COMPONENTS)
        set(_NTCP_COMPONENTS "")
        if(TARGET threadx_glue)
            list(APPEND _NTCP_COMPONENTS threadx_glue)
        endif()
        if(TARGET virtio_net_netx)
            list(APPEND _NTCP_COMPONENTS virtio_net_netx)
        endif()
        if(TARGET netxduo)
            list(APPEND _NTCP_COMPONENTS netxduo)
        endif()
        if(TARGET nsos_netx)
            list(APPEND _NTCP_COMPONENTS nsos_netx)
        endif()
        list(APPEND _NTCP_COMPONENTS threadx_kernel)
    endif()

    nros_compose_platform_target(threadx_platform
        COMPONENTS ${_NTCP_COMPONENTS}
        LINK_LIBS  ${_NTCP_LINK_LIBS}
        INCLUDES   ${NROS_THREADX_INCLUDES}
        DEFINES    ${_NTCP_DEFINES})

    if(_NTCP_LINK_OPTIONS)
        target_link_options(threadx_platform INTERFACE ${_NTCP_LINK_OPTIONS})
    endif()
endfunction()

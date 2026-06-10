# cmake/board/nano-ros-board-nuttx-qemu-riscv.cmake
#
# 194.3c — board overlay for QEMU rv-virt (rv32imac) under NuttX. Mirror of
# nano-ros-board-nuttx-qemu-arm.cmake with the riscv FFI crate, riscv cargo
# target, the rv-virt defconfig, and the rv-virt Make.defs path.
#
# Loaded by cmake/platform/nano-ros-nuttx.cmake when NANO_ROS_BOARD=nuttx-qemu-riscv.
#
# Required cmake variables (env or -D):
#   NUTTX_DIR        — NuttX kernel source/build tree (third-party/nuttx/nuttx/)
#   NUTTX_APPS_DIR   — optional, defaults to <NUTTX_DIR>/../nuttx-apps
#
# See the arm overlay for the rationale behind nros_board_link_app (the linked
# binary IS the NuttX kernel; the relink runs inside cargo on the FFI crate).

if(DEFINED _NROS_BOARD_NUTTX_QEMU_RISCV_INCLUDED)
    return()
endif()
set(_NROS_BOARD_NUTTX_QEMU_RISCV_INCLUDED TRUE)

set(_NROS_BOARD_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_NUTTX_BOARD_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-nuttx-qemu-riscv")
set(_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT
    "${_NROS_NUTTX_BOARD_DIR}/nros-nuttx-ffi")

# 194.4 self-provision + 194.3c riscv inputs: the board supplies its defconfig
# AND its per-arch Make.defs path to the shared provisioning script.
set(NROS_NUTTX_PROVISION_SCRIPT "${_NROS_BOARD_ROOT}/scripts/nuttx/build-nuttx.sh"
    CACHE FILEPATH "NuttX export provisioning script (make export), run before the example build")
set(NROS_NUTTX_DEFCONFIG "${_NROS_NUTTX_BOARD_DIR}/nuttx-config/defconfig"
    CACHE FILEPATH "Board NuttX defconfig consumed by the provisioning script")
set(NROS_NUTTX_BOARD_MAKEDEFS "boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs"
    CACHE STRING "Board Make.defs path (relative to NUTTX_DIR) for the provisioning script")

if(NOT DEFINED NUTTX_FFI_CRATE_DIR AND DEFINED ENV{NUTTX_FFI_CRATE_DIR})
    set(NUTTX_FFI_CRATE_DIR "$ENV{NUTTX_FFI_CRATE_DIR}")
endif()
if(NOT DEFINED NUTTX_FFI_CRATE_DIR)
    set(NUTTX_FFI_CRATE_DIR "${_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT}"
        CACHE PATH "Path to nros-nuttx-ffi crate (NuttX kernel + FFI bundle)")
endif()
if(NOT EXISTS "${NUTTX_FFI_CRATE_DIR}/Cargo.toml")
    message(FATAL_ERROR
        "nano-ros-board-nuttx-qemu-riscv: NUTTX_FFI_CRATE_DIR points at "
        "'${NUTTX_FFI_CRATE_DIR}' but Cargo.toml is missing. Default "
        "in-tree path: ${_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT}.")
endif()

set(NUTTX_BOARD_TOOLCHAIN_FILE
    "${_NROS_NUTTX_BOARD_DIR}/riscv-nuttx-toolchain.cmake"
    CACHE FILEPATH "Optional CMake toolchain file for riscv-none-elf cross.")

nros_nuttx_validate(REQUIRE NUTTX_DIR)
nros_nuttx_set_cargo_target("riscv32imac-unknown-nuttx-elf")

if(NOT DEFINED CACHE{Rust_CARGO_TARGET})
    set(Rust_CARGO_TARGET "riscv32imac-unknown-nuttx-elf" CACHE STRING
        "Rust cargo target triple (set by NuttX riscv board overlay)")
endif()

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>) — identical logic to the arm overlay; pulls the
# carrier add_executable's SOURCES/INCLUDE_DIRECTORIES/LINK_LIBRARIES and feeds
# nros_nuttx_build_example() with the riscv target triple.
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()

    get_target_property(_srcs ${target} SOURCES)
    if(NOT _srcs)
        message(FATAL_ERROR
            "nros_board_link_app(${target}): target has no SOURCES.")
    endif()
    set(_main_src "")
    set(_extra_srcs "")
    foreach(_s ${_srcs})
        if(NOT IS_ABSOLUTE "${_s}")
            get_target_property(_src_dir ${target} SOURCE_DIR)
            set(_s "${_src_dir}/${_s}")
        endif()
        if(NOT _main_src AND _s MATCHES "/(main|app)\\.(c|cc|cpp|cxx)$")
            set(_main_src "${_s}")
        else()
            list(APPEND _extra_srcs "${_s}")
        endif()
    endforeach()
    if(NOT _main_src)
        list(GET _srcs 0 _first)
        if(NOT IS_ABSOLUTE "${_first}")
            get_target_property(_src_dir ${target} SOURCE_DIR)
            set(_first "${_src_dir}/${_first}")
        endif()
        set(_main_src "${_first}")
        list(REMOVE_AT _srcs 0)
        set(_extra_srcs ${_srcs})
    endif()

    get_target_property(_incs ${target} INCLUDE_DIRECTORIES)
    if(NOT _incs)
        set(_incs "")
    endif()

    get_target_property(_libs ${target} LINK_LIBRARIES)
    set(_link_ifaces "")
    if(_libs)
        foreach(_lib ${_libs})
            if(_lib STREQUAL "NanoRos::NanoRos"
               OR _lib STREQUAL "NanoRos::NanoRosCpp"
               OR _lib STREQUAL "NanoRos"
               OR _lib STREQUAL "NanoRosCpp")
                if(TARGET ${_lib})
                    get_target_property(_nros_inc ${_lib} INTERFACE_INCLUDE_DIRECTORIES)
                    if(_nros_inc)
                        list(APPEND _incs ${_nros_inc})
                    endif()
                    get_target_property(_nros_link ${_lib} INTERFACE_LINK_LIBRARIES)
                    if(_nros_link)
                        foreach(_dep ${_nros_link})
                            if(TARGET ${_dep})
                                get_target_property(_dep_inc ${_dep}
                                    INTERFACE_INCLUDE_DIRECTORIES)
                                if(_dep_inc)
                                    list(APPEND _incs ${_dep_inc})
                                endif()
                            endif()
                        endforeach()
                    endif()
                endif()
                continue()
            endif()
            list(APPEND _link_ifaces "${_lib}")
        endforeach()
    endif()

    nros_nuttx_build_example(
        NAME            "${target}"
        MAIN_SOURCE     "${_main_src}"
        FFI_CRATE_DIR   "${NUTTX_FFI_CRATE_DIR}"
        TARGET_TRIPLE   "riscv32imac-unknown-nuttx-elf"
        INCLUDE_DIRS    ${_incs}
        SOURCES         ${_extra_srcs}
        LINK_INTERFACES ${_link_ifaces})

    set_target_properties(${target} PROPERTIES EXCLUDE_FROM_ALL TRUE)
    add_dependencies(${target} ${target}_build)
endfunction()

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

# RFC-0048 (phase-287 W5) — default NUTTX_DIR from the in-tree submodule so it
# leaves the `nros setup` preset's -D set. env / -D still override. Mirrors the
# arm board module + NUTTX_FFI_CRATE_DIR above.
if(NOT DEFINED NUTTX_DIR AND DEFINED ENV{NUTTX_DIR})
    set(NUTTX_DIR "$ENV{NUTTX_DIR}")
endif()
if(NOT DEFINED NUTTX_DIR)
    set(NUTTX_DIR "${_NROS_BOARD_ROOT}/third-party/nuttx/nuttx"
        CACHE PATH "NuttX kernel export tree (pre-built)")
endif()
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
    # phase-263 C2b — `<abs-src>=<pkg>` pairs for the per-component NuttX cc-rs compile.
    set(_source_pkgs "")
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
            # phase-263 C2b (ported from the arm overlay, #199) — a node component lib's
            # host-built `.a` is the WRONG ARCH for the NuttX kernel link; hand its
            # SOURCES to the cargo cc-rs build instead, each tagged with its pkg via
            # SOURCE_PKGS so cc-rs gives each its own `-DNROS_PKG_NAME`.
            if(TARGET ${_lib})
                get_target_property(_comp_pkg ${_lib} NROS_COMPONENT_PKG_SYM)
                if(_comp_pkg)
                    get_target_property(_comp_srcs ${_lib} SOURCES)
                    get_target_property(_comp_sdir ${_lib} SOURCE_DIR)
                    get_target_property(_comp_inc ${_lib} INTERFACE_INCLUDE_DIRECTORIES)
                    if(_comp_inc)
                        list(APPEND _incs ${_comp_inc})
                    endif()
                    if(_comp_srcs)
                        foreach(_cs ${_comp_srcs})
                            if(NOT IS_ABSOLUTE "${_cs}")
                                set(_cs "${_comp_sdir}/${_cs}")
                            endif()
                            list(APPEND _extra_srcs "${_cs}")
                            list(APPEND _source_pkgs "${_cs}=${_comp_pkg}")
                        endforeach()
                    endif()
                    # Issue 0149 — a LAUNCH-only workspace entry links only the
                    # component libs; the generated interface libs hang off the
                    # COMPONENT, so descend one level and pull them up.
                    get_target_property(_comp_links ${_lib} LINK_LIBRARIES)
                    if(_comp_links)
                        foreach(_cl ${_comp_links})
                            if(_cl MATCHES "__nano_ros_(c|cpp)$" AND TARGET ${_cl})
                                list(APPEND _link_ifaces "${_cl}")
                            endif()
                        endforeach()
                    endif()
                    continue()
                endif()
            endif()
            list(APPEND _link_ifaces "${_lib}")
        endforeach()
    endif()
    if(_link_ifaces)
        list(REMOVE_DUPLICATES _link_ifaces)
    endif()

    # phase-281 W3-nuttx C lane (ported from the arm overlay, #199) — a TYPED C
    # node's generated interface lib `<pkg>__nano_ros_c` is a HOST-arch static lib
    # of serdes `.c` TUs (`std_msgs_msg_string_{init,serialize,get_type_support}`
    # etc.), so its `.a` can never join the riscv kernel link and — unlike the C++
    # lane — there is no cross-compiled `<lib>_ffi_lib`. Hand the generated `.c`
    # SOURCES to the cc-rs cross-compile via the dedicated INTERFACE_SOURCES
    # channel, which lands them in a TRAILING `app_iface` archive linked AFTER the
    # node archives (the node TUs reference the serdes, so the defining archive
    # must come later on the single-pass link line). Without this walk the image
    # link dies with `undefined reference to std_msgs_msg_string_serialize`.
    set(_iface_srcs "")
    foreach(_iface ${_link_ifaces})
        if(_iface MATCHES "__nano_ros_c$" AND TARGET ${_iface})
            get_target_property(_iface_type ${_iface} TYPE)
            if(_iface_type STREQUAL "STATIC_LIBRARY")
                get_target_property(_iface_lib_srcs ${_iface} SOURCES)
                get_target_property(_iface_sdir ${_iface} SOURCE_DIR)
                if(_iface_lib_srcs)
                    foreach(_is ${_iface_lib_srcs})
                        if(NOT IS_ABSOLUTE "${_is}")
                            set(_is "${_iface_sdir}/${_is}")
                        endif()
                        list(APPEND _iface_srcs "${_is}")
                    endforeach()
                endif()
            endif()
        endif()
    endforeach()
    if(_iface_srcs)
        list(REMOVE_DUPLICATES _iface_srcs)
    endif()

    # Phase 238 ferry (ported from the arm board cmake, #134 follow-up):
    # carry the carrier's COMPILE_DEFINITIONS into the cargo cc-rs build so
    # EXTRA_SOURCES (the declarative Component `Talker.c` etc.) see
    # `NROS_PKG_NAME` — without it the component registers as
    # `__nros_c_component_NROS_PKG_NAME_*` and the generated entry's
    # `__nros_c_component_<pkg>_*` references fail to link.
    get_target_property(_cdefs ${target} COMPILE_DEFINITIONS)
    set(_compile_defs "")
    if(_cdefs)
        set(_compile_defs ${_cdefs})
    endif()

    nros_nuttx_build_example(
        NAME            "${target}"
        MAIN_SOURCE     "${_main_src}"
        FFI_CRATE_DIR   "${NUTTX_FFI_CRATE_DIR}"
        TARGET_TRIPLE   "riscv32imac-unknown-nuttx-elf"
        INCLUDE_DIRS    ${_incs}
        SOURCES         ${_extra_srcs}
        SOURCE_PKGS     ${_source_pkgs}
        INTERFACE_SOURCES ${_iface_srcs}
        COMPILE_DEFS    ${_compile_defs}
        LINK_INTERFACES ${_link_ifaces})

    set_target_properties(${target} PROPERTIES EXCLUDE_FROM_ALL TRUE)
    add_dependencies(${target} ${target}_build)
endfunction()

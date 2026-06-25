# cmake/board/nano-ros-board-nuttx-qemu-arm.cmake
#
# Phase 138.3 / 144.6 — board overlay for QEMU ARM virt (Cortex-A7)
# under NuttX. Mirrors the legacy
# `packages/core/nros-c/cmake/nuttx-support.cmake` shape, with the
# FFI-crate path pointed at the in-tree (Phase-144.6-relocated)
# location under `packages/boards/nros-board-nuttx-qemu-arm/`.
#
# Loaded by `cmake/platform/nano-ros-nuttx.cmake` when
# NANO_ROS_BOARD=nuttx-qemu-arm.
#
# Required cmake variables (env or -D):
#   NUTTX_DIR        — pre-built NuttX kernel source/build tree
#                      (typically `third-party/nuttx/nuttx/`)
#   NUTTX_APPS_DIR   — optional, defaults to `<NUTTX_DIR>/../nuttx-apps`
#
# What this overlay does:
#
#   * Sets `NUTTX_FFI_CRATE_DIR` to the in-tree
#     `packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi/`
#     unless already set by the caller.
#
#   * Validates NUTTX_DIR via `nros_nuttx_validate`.
#
#   * Pins the cargo target triple to `armv7a-nuttx-eabihf` (matches
#     NuttX QEMU virt board / hardfloat ABI used by all NuttX libs).
#
# Function:
#   nros_board_link_app(<target>)
#       Per-app fixup invoked by nros_platform_link_app(<target>).
#       Reads <target>'s SOURCES + LINK_LIBRARIES + INCLUDE_DIRECTORIES
#       and redirects them through `nuttx_build_example(<target>
#       <main_src> INCLUDE_DIRS ... SOURCES ... LINK_INTERFACES ...)`.
#       The cargo custom_target emits the real NuttX kernel ELF
#       (`<build>/<target>`); the CMake `add_executable` target is
#       left as a vestigial stub for downstream tooling that walks
#       the CMake graph by name.
#
# Why this shape:
#
#   NuttX's build is fundamentally not a `target_link_libraries`
#   composition — the linked binary IS the NuttX kernel image, and
#   the relink runs inside `cargo build` on `nros-nuttx-ffi` so
#   `cargo`/`rustc`-emitted code (Rust main + zenoh-pico Rust glue +
#   Rust panic handler) sit alongside the C/C++ app code. Translating
#   the Phase 144.5 `add_executable + target_link_libraries +
#   nros_platform_link_app` contract onto NuttX therefore means:
#   accept the user's `add_executable` as a declarative carrier of
#   `<name>`, `<main_src>`, and the codegen interface libs, then
#   redispatch through `nuttx_build_example`.

if(DEFINED _NROS_BOARD_NUTTX_QEMU_ARM_INCLUDED)
    return()
endif()
set(_NROS_BOARD_NUTTX_QEMU_ARM_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# Resolve the in-tree FFI-crate path. The crate lives next to this
# overlay's package (Phase 144.6 relocation); callers can still
# override via env var / -D for out-of-tree FFI bundles.
# ---------------------------------------------------------------------------
set(_NROS_BOARD_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_NUTTX_BOARD_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-nuttx-qemu-arm")
set(_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT
    "${_NROS_NUTTX_BOARD_DIR}/nros-nuttx-ffi")

# 194.4: self-provision the NuttX export. nros_nuttx_build_example runs this
# (idempotent — the marker self-guards) before the example cargo build, so
# `nros build`/`deploy` + raw cmake auto-build the NuttX export. The provisioning
# script lives in the shared build-script dir (`scripts/nuttx/`) so the builders
# are self-contained; the board supplies its own defconfig via NROS_NUTTX_DEFCONFIG
# (a new-arch board overrides the defconfig, reusing the shared script).
set(NROS_NUTTX_PROVISION_SCRIPT "${_NROS_BOARD_ROOT}/scripts/nuttx/build-nuttx.sh"
    CACHE FILEPATH "NuttX export provisioning script (make export), run before the example build")
set(NROS_NUTTX_DEFCONFIG "${_NROS_NUTTX_BOARD_DIR}/nuttx-config/defconfig"
    CACHE FILEPATH "Board NuttX defconfig consumed by the provisioning script")

if(NOT DEFINED NUTTX_FFI_CRATE_DIR AND DEFINED ENV{NUTTX_FFI_CRATE_DIR})
    set(NUTTX_FFI_CRATE_DIR "$ENV{NUTTX_FFI_CRATE_DIR}")
endif()
if(NOT DEFINED NUTTX_FFI_CRATE_DIR)
    set(NUTTX_FFI_CRATE_DIR "${_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT}"
        CACHE PATH "Path to nros-nuttx-ffi crate (NuttX kernel + FFI bundle)")
endif()
if(NOT EXISTS "${NUTTX_FFI_CRATE_DIR}/Cargo.toml")
    message(FATAL_ERROR
        "nano-ros-board-nuttx-qemu-arm: NUTTX_FFI_CRATE_DIR points at "
        "'${NUTTX_FFI_CRATE_DIR}' but Cargo.toml is missing. Default "
        "in-tree path: ${_NROS_NUTTX_FFI_CRATE_DIR_DEFAULT}.")
endif()

# Expose the optional toolchain file under the same package — users
# pass `-DCMAKE_TOOLCHAIN_FILE=$NUTTX_BOARD_TOOLCHAIN_FILE` if they
# want CMake itself to cross to ARM, but most of the time cargo
# drives the cross via build.rs and CMake is host-mode.
set(NUTTX_BOARD_TOOLCHAIN_FILE
    "${_NROS_NUTTX_BOARD_DIR}/armv7a-nuttx-toolchain.cmake"
    CACHE FILEPATH "Optional CMake toolchain file for arm-none-eabi cross.")

# ---------------------------------------------------------------------------
# Validate NUTTX_DIR + pin the cargo target triple.
# ---------------------------------------------------------------------------
nros_nuttx_validate(REQUIRE NUTTX_DIR)
nros_nuttx_set_cargo_target("armv7a-nuttx-eabihf")

# Phase 156 (F3) — also publish Rust_CARGO_TARGET as CACHE so it
# reaches scopes that PARENT_SCOPE can't cross (notably the example
# CMakeLists.txt that add_subdirectory'd this root). The codegen
# pipeline at `NanoRosGenerateInterfaces.cmake:466` reads
# Rust_CARGO_TARGET to enable the `+nightly` / `-Zbuild-std=core`
# path for tier-3 NuttX cargo invocations. Safe to set here AFTER
# the corrosion `add_subdirectory(nros-c/nros-cpp)` calls in the
# root CMakeLists.txt have already run (those run before
# `cmake/platform/nano-ros-nuttx.cmake` is included, so corrosion
# still sees the unset value and builds for host — which is
# discarded later because the real ELF link goes through
# `nros_nuttx_build_example`'s cargo invocation that cross-builds
# every nros-* crate via the FFI crate's path-deps).
if(NOT DEFINED CACHE{Rust_CARGO_TARGET})
    set(Rust_CARGO_TARGET "armv7a-nuttx-eabihf" CACHE STRING
        "Rust cargo target triple (Phase 156 F3 — set by NuttX board overlay)")
endif()

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>)
#
# Pull the carrier add_executable target's properties off and feed
# them into nros_nuttx_build_example(). The cargo build emits the
# real ELF at <build>/<target>; the CMake target itself stays as a
# stub. The user's `target_link_libraries(<target> PRIVATE
# std_msgs__nano_ros_c NanoRos::NanoRos)` plus any
# `target_include_directories(<target> PRIVATE ...)` calls are read
# back via target properties.
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()

    # Pull SOURCES — the first non-generated source is the example's
    # main.c / main.cpp.
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
        # Fall back to the first source.
        list(GET _srcs 0 _first)
        if(NOT IS_ABSOLUTE "${_first}")
            get_target_property(_src_dir ${target} SOURCE_DIR)
            set(_first "${_src_dir}/${_first}")
        endif()
        set(_main_src "${_first}")
        list(REMOVE_AT _srcs 0)
        set(_extra_srcs ${_srcs})
    endif()

    # Pull include directories.
    get_target_property(_incs ${target} INCLUDE_DIRECTORIES)
    if(NOT _incs)
        set(_incs "")
    endif()

    # Pull link libraries — the codegen interface libs
    # (`<pkg>__nano_ros_c`, `<pkg>__nano_ros_cpp`) and NanoRos::NanoRos
    # are routed into nuttx_build_example as LINK_INTERFACES, which
    # walks each lib's INTERFACE_INCLUDE_DIRECTORIES + per-package
    # `*_ffi_lib` static archive into the cargo build. Skip
    # NanoRos::NanoRos itself for the LINK_INTERFACES list: cargo
    # drags the NanoRos transitive closure (nros-c / nros-cpp /
    # nros-rmw-zenoh / etc.) in via the FFI crate's
    # `[dependencies]` table.
    #
    # Phase 155.B.5 — but NanoRos's INTERFACE_INCLUDE_DIRECTORIES
    # carries the per-build mirror dir for `nros_config_generated.h`
    # (`${CMAKE_CURRENT_BINARY_DIR}/include`, set by nros-c's
    # CMakeLists). Cargo can't discover that location, so the
    # `nros-c/include/nros_config_generated.h` stub
    # (which `#error`s) wins. Pull NanoRos's INTERFACE_INCLUDE_DIRECTORIES
    # into INCLUDE_DIRS separately so the mirror dir reaches the FFI
    # cc-rs build before the source-tree fallback.
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
                # Skip the link target but ferry its include dirs.
                if(TARGET ${_lib})
                    get_target_property(_nros_inc ${_lib} INTERFACE_INCLUDE_DIRECTORIES)
                    if(_nros_inc)
                        list(APPEND _incs ${_nros_inc})
                    endif()
                    # Also walk the umbrella's transitive
                    # INTERFACE_LINK_LIBRARIES (nros_c-static,
                    # nros_cpp-static) — those carry the mirror
                    # dirs set by `nros-c/CMakeLists.txt:128`.
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
            # phase-263 C2b — a node component lib (a `<pkg>_<exec>_component` static lib
            # carrying `NROS_C_COMPONENT` sources, each compiled with its own
            # `-DNROS_PKG_NAME`). The host-built `.a` is the WRONG ARCH for the NuttX kernel
            # link (`armv7a-nuttx-eabihf` → "file format not recognized"), so do NOT link it.
            # Hand its SOURCES to the cargo cc-rs build to recompile for the ARM target, each
            # tagged with its pkg via `SOURCE_PKGS` (→ APP_EXTRA_SOURCE_PKGS), so the cc-rs
            # build gives each its own `-DNROS_PKG_NAME` (a single archive can carry only one).
            # This is what makes multi-node C work on NuttX (cf. Zephyr's separate static libs).
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
                    continue()
                endif()
            endif()
            list(APPEND _link_ifaces "${_lib}")
        endforeach()
    endif()

    # Phase 238 — ferry the carrier's COMPILE_DEFINITIONS into the cargo
    # cc-rs build so EXTRA_SOURCES (e.g. a declarative Component class
    # `Talker.cpp`) see `NROS_PKG_NAME` — required by the
    # `NROS_NODE_REGISTER` macro to emit the per-pkg
    # `__nros_component_<pkg>_register` symbol the generated entry calls.
    get_target_property(_cdefs ${target} COMPILE_DEFINITIONS)
    set(_compile_defs "")
    if(_cdefs)
        set(_compile_defs ${_cdefs})
    endif()

    nros_nuttx_build_example(
        NAME            "${target}"
        MAIN_SOURCE     "${_main_src}"
        FFI_CRATE_DIR   "${NUTTX_FFI_CRATE_DIR}"
        TARGET_TRIPLE   "armv7a-nuttx-eabihf"
        INCLUDE_DIRS    ${_incs}
        SOURCES         ${_extra_srcs}
        SOURCE_PKGS     ${_source_pkgs}
        COMPILE_DEFS    ${_compile_defs}
        LINK_INTERFACES ${_link_ifaces})

    # Phase 156 (NuttX) — neutralise the carrier `add_executable`
    # target. The real ELF is emitted by `<target>_build`'s cargo
    # invocation (NuttX kernel link via arm-none-eabi-gcc); the
    # carrier was kept only as a declarative `target_link_libraries`
    # / `target_include_directories` sink. Without these props, CMake
    # tries to link the carrier with the *host* toolchain
    # (x86_64-linux-gnu gcc), which fails with
    # `undefined reference to 'main'` because the NuttX example
    # registers `void app_main(void)` via NROS_APP_MAIN_REGISTER_VOID,
    # not `int main`.
    #
    #   EXCLUDE_FROM_ALL — keep it out of the default build
    #   add_dependencies(carrier <name>_build) — `cmake --build . --target <name>`
    #     still produces the kernel ELF via the cargo path
    set_target_properties(${target} PROPERTIES EXCLUDE_FROM_ALL TRUE)
    add_dependencies(${target} ${target}_build)
endfunction()

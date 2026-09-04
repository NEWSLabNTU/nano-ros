# nros-nuttx.cmake
#
# Per-RTOS cmake module for NuttX. Phase 91.E1c: NuttX has its own
# native build system (kconfig + make), so cmake's job is **not** to
# rebuild the kernel. Instead, the per-example "build" is a delegating
# `cargo build` that drives `nros-nuttx-ffi` (a Rust crate whose
# build.rs invokes the NuttX toolchain on the user's main.c/main.cpp
# plus codegen-generated sources, and links against the NuttX
# pre-built libraries via NUTTX_DIR / NUTTX_APPS_DIR).
#
# This module captures the cmake → cargo plumbing that every NuttX
# port needs: include-dir closure, FFI staticlib closure, env-var
# wiring, per-example cargo target dir to avoid cross-binary
# clobbering, post-build copy. New NuttX ports (RISC-V, AArch64, …)
# pick a different TARGET_TRIPLE; the rest of the function body
# stays.
#
# Public functions:
#
#   nros_nuttx_validate(REQUIRE <vars…>)
#       Validate the listed cmake variables (env-or-fatal-error).
#       Always requires NUTTX_DIR plus whatever the caller passes in
#       REQUIRE. Defaults NUTTX_APPS_DIR to "${NUTTX_DIR}/../nuttx-apps"
#       if not provided.
#
#   nros_nuttx_set_cargo_target(<triple>)
#       Sets the parent-scope `Rust_CARGO_TARGET` so the codegen
#       pipeline's per-package FFI staticlibs cross-compile to the
#       same target as the example ELF. Without this they get built
#       for the host triple and the leaf NuttX link fails with
#       `file format not recognized`.
#
#   nros_nuttx_build_example(NAME <name>
#                            MAIN_SOURCE <c-or-cpp-file>
#                            FFI_CRATE_DIR <path>
#                            TARGET_TRIPLE <triple>
#                            [INCLUDE_DIRS <dirs…>]
#                            [SOURCES <extra-c-files…>]
#                            [COMPILE_DEFS <defs…>]
#                            [LINK_INTERFACES <codegen-libs…>])
#       Schedules a `cargo build` of the FFI crate at `FFI_CRATE_DIR` at
#       the `nuttx-rust` CARVE-OUT profile — NOT `--release`, which this
#       docstring claimed after issue 0820 changed the code beneath it.
#       Cargo's built-in `release` is `lto = "off"`, and at `lto = off` a
#       cross-CGU miscompile corrupts std's `lang_start` closure and the
#       image reboots before `main` with no console output
#       (phase-177.8.c). The carve-out exists to prevent exactly that.
#       Env vars wire the user's main +
#       includes + extra sources + compile defs + FFI staticlibs into
#       the crate's build.rs. Produces an ELF at
#       <build>/<NAME>. Each LINK_INTERFACES entry's
#       INTERFACE_INCLUDE_DIRECTORIES (resolved via file(GENERATE)
#       so generator expressions survive the cmake → cargo handoff)
#       and per-package _ffi_lib are pulled in transitively.

if(DEFINED _NROS_NUTTX_INCLUDED)
    return()
endif()
set(_NROS_NUTTX_INCLUDED TRUE)

include("${CMAKE_CURRENT_LIST_DIR}/nros-rtos-helpers.cmake")

# The `_NROS_ENTRY_DIR` pattern from CLAUDE.md, and it is not optional here:
# measured, `CMAKE_CURRENT_LIST_DIR` inside a FUNCTION body resolves to the
# CALLER's list dir, not to the file the function was defined in. The depfile
# script below is invoked from inside `nros_nuttx_build_example`, so its path
# has to be captured at FILE scope.
set(_NROS_NUTTX_CMAKE_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE INTERNAL
    "Directory holding nros-nuttx.cmake and its helper scripts")

# issue 0820 — the cargo profile for this seam is a CARVE-OUT, not the ambient
# one, and it must come from the table rather than be spelled here.
#
# At FILE scope, not inside the function that uses it: an include() in a
# function body drops the included file's normal variables when the frame pops
# (the `_NROS_ENTRY_DIR` class in CLAUDE.md). NanoRosCargoProfile.cmake carries
# include_guard(GLOBAL), so this is a no-op when a caller already included it.
if(NOT COMMAND nros_resolve_carve_out_profile)
    get_filename_component(_nros_c_repo_cmake
        "${CMAKE_CURRENT_LIST_DIR}/../../../../cmake" ABSOLUTE)
    if(EXISTS "${_nros_c_repo_cmake}/NanoRosCargoProfile.cmake")
        include("${_nros_c_repo_cmake}/NanoRosCargoProfile.cmake")
    endif()
endif()

# ----------------------------------------------------------------------
# nros_nuttx_validate
# ----------------------------------------------------------------------
# Issue 0689 — this lane applies the image's ending ITSELF, so
# `nano_ros_entry()` must verify rather than scan.
#
# The Rust side here is an `add_custom_target` cargo build of `nros-nuttx-ffi`,
# not a Corrosion target, so there is nothing for `corrosion_set_features()` to
# act on — the same shape the Zephyr lane has. The ending is baked into
# `nros-nuttx-ffi`'s COMMITTED manifest, which names `nros-c`'s features
# directly:
#
#     nros-c = { path = …, default-features = false, features = [
#         "alloc", "global-allocator", "panic-platform", … ] }
#
# so this lane supports exactly one policy. Declaring it lets an entry that asks
# for a DIFFERENT one fail with that sentence instead of "no Rust target exists",
# which described the mechanism rather than the cause.
set_property(GLOBAL PROPERTY NROS_ENTRY_PANIC_APPLIED "platform")
set_property(GLOBAL PROPERTY NROS_ENTRY_PANIC_APPLIED_BY "NuttX")
set_property(GLOBAL PROPERTY NROS_ENTRY_PANIC_APPLIED_HOW
    "On NuttX the ending is baked into `nros-nuttx-ffi`'s committed manifest \
(`nros-c` features include `panic-platform`), because the staticlib is built by \
a custom cargo target rather than Corrosion. This lane can only offer PANIC \
platform; change the manifest to offer another.")

function(nros_nuttx_validate)
    cmake_parse_arguments(_NNV "" "" "REQUIRE" ${ARGN})
    nros_validate_vars(NUTTX_DIR ${_NNV_REQUIRE})

    if(NOT DEFINED NUTTX_APPS_DIR)
        if(DEFINED ENV{NUTTX_APPS_DIR})
            set(NUTTX_APPS_DIR "$ENV{NUTTX_APPS_DIR}")
        else()
            set(NUTTX_APPS_DIR "${NUTTX_DIR}/../nuttx-apps")
        endif()
    endif()

    set(NUTTX_DIR      "${NUTTX_DIR}"      PARENT_SCOPE)
    set(NUTTX_APPS_DIR "${NUTTX_APPS_DIR}" PARENT_SCOPE)
    foreach(_v ${_NNV_REQUIRE})
        set(${_v} "${${_v}}" PARENT_SCOPE)
    endforeach()
endfunction()

# ----------------------------------------------------------------------
# nros_nuttx_set_cargo_target
# ----------------------------------------------------------------------
function(nros_nuttx_set_cargo_target triple)
    if(NOT DEFINED Rust_CARGO_TARGET)
        set(Rust_CARGO_TARGET "${triple}" PARENT_SCOPE)
    endif()
endfunction()

# ----------------------------------------------------------------------
# nros_nuttx_build_example
# ----------------------------------------------------------------------
function(nros_nuttx_build_example)
    cmake_parse_arguments(_NNBE
        ""
        "NAME;MAIN_SOURCE;FFI_CRATE_DIR;TARGET_TRIPLE"
        "INCLUDE_DIRS;SOURCES;SOURCE_PKGS;INTERFACE_SOURCES;COMPILE_DEFS;LINK_INTERFACES"
        ${ARGN})

    foreach(_req NAME MAIN_SOURCE FFI_CRATE_DIR TARGET_TRIPLE)
        if(NOT _NNBE_${_req})
            message(FATAL_ERROR
                "nros_nuttx_build_example: ${_req} is required.")
        endif()
    endforeach()

    # NanoRos_DIR (set by find_package(NanoRos CONFIG)) points at
    # `<prefix>/lib/cmake/NanoRos/` — `${NanoRos_DIR}/../../../include`
    # resolves to `<prefix>/include`. Under the Phase 137
    # `add_subdirectory(<repo>)` shape there is no NanoRos_DIR; fall
    # back to `${_NANO_ROS_PREFIX}/packages/api/nros-cpp/include`
    # (the in-tree source layout). `_NANO_ROS_PREFIX` is set by the
    # platform module to the repo root.
    if(DEFINED NanoRos_DIR)
        get_filename_component(_nros_cpp_include
            "${NanoRos_DIR}/../../../include" ABSOLUTE)
    elseif(DEFINED _NANO_ROS_PREFIX)
        get_filename_component(_nros_cpp_include
            "${_NANO_ROS_PREFIX}/packages/api/nros-cpp/include" ABSOLUTE)
    else()
        message(FATAL_ERROR
            "nros_nuttx_build_example: neither NanoRos_DIR (legacy "
            "find_package shape) nor _NANO_ROS_PREFIX (Phase 137 "
            "add_subdirectory shape) is defined. The platform module "
            "should set one of them before calling this function.")
    endif()

    # ── include-dir closure via file(GENERATE) ────────────────────────
    # Each LINK_INTERFACES library's INTERFACE_INCLUDE_DIRECTORIES is a
    # generator expression that resolves to a list. Semicolons don't
    # round-trip through `cmake -E env` cleanly (either explode into
    # separate args or pass through verbatim and confuse cargo). We
    # materialise the closure to a sentinel file and let build.rs read
    # it. cmake walks INTERFACE_LINK_LIBRARIES transitively, so each
    # leaf `<pkg>__nano_ros_cpp` library's include closure flows in
    # automatically.
    set(_static_includes "${_nros_cpp_include}")
    foreach(_dir ${_NNBE_INCLUDE_DIRS})
        list(APPEND _static_includes "${_dir}")
    endforeach()
    set(_includes_file "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}_includes.txt")
    set(_iface_genex_lines "")
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        list(APPEND _iface_genex_lines
            "$<JOIN:$<TARGET_PROPERTY:${_lib},INTERFACE_INCLUDE_DIRECTORIES>,\n>")
    endforeach()
    set(_static_block "")
    foreach(_dir ${_static_includes})
        string(APPEND _static_block "${_dir}\n")
    endforeach()
    file(GENERATE
        OUTPUT "${_includes_file}"
        CONTENT "${_static_block}$<JOIN:${_iface_genex_lines},\n>\n")

    # ── FFI staticlib closure ─────────────────────────────────────────
    # The codegen pipeline builds one `<leaf>__nano_ros_cpp_ffi_lib`
    # per package, and that crate's lib.rs `include!()`s the FFI Rust
    # glue from every transitive dep (see NanoRosGenerateInterfaces.
    # cmake). Linking the leaves transitively pulls in all dep types;
    # dep packages' own `*_ffi_lib` static libs aren't built (and
    # aren't needed). Only `<pkg>__nano_ros_cpp_gen` runs for each
    # transitive dep so the codegen .hpp/.rs files exist for
    # `include!()`.
    set(_ffi_libs_file "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}_ffi_libs.txt")
    set(_ffi_lib_lines "")
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        if(TARGET ${_lib}_ffi_lib)
            list(APPEND _ffi_lib_lines "$<TARGET_FILE:${_lib}_ffi_lib>")
        endif()
    endforeach()
    if(_ffi_lib_lines)
        file(GENERATE
            OUTPUT "${_ffi_libs_file}"
            CONTENT "$<JOIN:${_ffi_lib_lines},\n>\n")
    else()
        file(GENERATE OUTPUT "${_ffi_libs_file}" CONTENT "")
    endif()

    # ── extra sources + compile defs (semicolon-joined) ───────────────
    set(_extra_sources "")
    foreach(_src ${_NNBE_SOURCES})
        list(APPEND _extra_sources "${_src}")
    endforeach()
    string(JOIN ";" _extra_sources_str ${_extra_sources})

    # phase-263 C2b — per-component `<abs-src>=<pkg>` map → APP_EXTRA_SOURCE_PKGS, so the
    # cc-rs build compiles each component source with its OWN `-DNROS_PKG_NAME`.
    string(JOIN ";" _source_pkgs_str ${_NNBE_SOURCE_PKGS})

    # phase-281 W3-nuttx (C lane) — generated C interface serdes TUs → APP_INTERFACE_SOURCES,
    # which the FFI build.rs compiles into a TRAILING `app_iface` archive linked AFTER the
    # per-node `app_pkg_*` archives (the node TUs REFERENCE these serdes, so their defining
    # archive must come later on the single-pass link line). See the board overlay's
    # `nros_board_link_app` for why the C serdes can't ride the C++ `_ffi_lib` path.
    string(JOIN ";" _iface_sources_str ${_NNBE_INTERFACE_SOURCES})

    set(_compile_defs "")
    foreach(_def ${_NNBE_COMPILE_DEFS})
        list(APPEND _compile_defs "${_def}")
    endforeach()
    string(JOIN ";" _compile_defs_str ${_compile_defs})


    # issue 0820 — the profile is a CARVE-OUT (`nuttx-rust`), not `--release`.
    #
    # `NUTTX_RUST_PROFILE` is `nros-minsizerel` and its docstring is emphatic
    # about why: at `lto = "off"` a non-deterministic cross-CGU miscompile
    # corrupts the std `lang_start` main-closure fat pointer and the image
    # reboots before `main` with no console output (phase-177.8.c; phase-285 W5
    # rode the same dodge for nuttx-riscv). Cargo's built-in `release` IS
    # `lto = off`, so the hardcoded `cargo build --release` this replaced put
    # every NuttX C example squarely in the miscompiling configuration, and
    # built it at `release` codegen on a platform that chose minsizerel for size.
    #
    # The path moves WITH the profile. It was `.../release/...` literally, which
    # made the hardcode load-bearing: any other profile writes elsewhere while
    # cmake still expects `release/`, and the guard below reports "produced no
    # kernel ELF" instead.
    nros_resolve_carve_out_profile(nuttx-rust _NROS_NUTTX)

    # Placed AFTER `nros_resolve_carve_out_profile` deliberately: that call is
    # what defines `_NROS_NUTTX_PROFILE`, which the key below hashes. Computing
    # a key before its inputs exist is the exact defect this mechanism hit on
    # the native lane — the profile read empty on a fresh configure and
    # `release` on the next, so one leaf produced two keys (issue 0805).
    # ── cargo target dir: per-example, or SHARED when the lane asks ───
    # Without a per-example dir every example's cargo build lands at the same
    # path under the FFI crate's `target/`, and concurrent / sequential builds
    # from different examples silently clobber each other.
    #
    # issue 0805 — that clobber argument is about the FINAL artifact, and it is
    # correct: the per-example `nros-nuttx-ffi` binaries genuinely DIFFER
    # (measured: three leaves, three distinct hashes), because build.rs compiles
    # each app's own C sources in. What does NOT differ is everything else, and
    # that is where the mass is:
    #
    #     nros-nuttx-ffi      736 KB   per-example
    #     the rest           715 MB   deps + build scripts, identical
    #
    # 13 leaves x ~716 MB is ~9 GB of the same dependency graph. So share the
    # target dir and keep the two per-example outputs out of it:
    #
    #   * the ARTIFACT via cargo's own `--artifact-dir`, so cargo places it
    #     rather than a copy racing whatever runs next;
    #   * the DEPFILE by RETARGETING it in the same command, immediately after
    #     — a plain copy keeps cargo's own path as the rule's target, which
    #     ninja rejects and which cost the seam its whole rebuild edge again
    #     (see the `_depfile_retarget_cmd` note below).
    #
    # The key MUST separate the architectures. `<target>/release/build/` holds
    # HOST build-script output and is NOT triple-separated inside one target
    # dir, while NuttX's kernel tree is reconfigured in place between arm and
    # rv-virt — so a dir shared across arches would serve build-script output
    # compiled against the other arch's headers. Triple + profile + FFI crate
    # keeps them apart (the two arches also use different FFI crates).
    set(_shared_cargo_dir "")
    if(COMMAND nros_shared_cargo_dir)
        nros_shared_cargo_dir(_shared_cargo_dir KEY
            "triple=${_NNBE_TARGET_TRIPLE}"
            "profile=${_NROS_NUTTX_PROFILE}"
            "ffi=${_NNBE_FFI_CRATE_DIR}"
            "nuttx=${NUTTX_DIR}"
            "defconfig=${NROS_NUTTX_DEFCONFIG}")
    endif()
    if(_shared_cargo_dir)
        set(_cargo_target_dir "${_shared_cargo_dir}")
        set(_artifact_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-nuttx-ffi-out")
        file(MAKE_DIRECTORY "${_artifact_dir}")
    else()
        set(_cargo_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo-target")
        set(_artifact_dir "")
    endif()
    # Where cargo itself writes the artifact and its depfile.
    set(_cargo_out_dir
        "${_cargo_target_dir}/${_NNBE_TARGET_TRIPLE}/${_NROS_NUTTX_DIR}")
    if(_artifact_dir)
        # Shared target dir: the two PER-EXAMPLE outputs must leave it, or leaf
        # N overwrites leaf N-1. The depfile is per-example too — measured: it
        # names this leaf's own `*_includes.txt`, `*_ffi_libs.txt` and
        # `src/main.c` — so a shared one would hand every other leaf the wrong
        # rebuild triggers, which is issue 0820's museum-binary failure.
        set(_output_binary "${_artifact_dir}/nros-nuttx-ffi")
    else()
        set(_output_binary "${_cargo_out_dir}/nros-nuttx-ffi")
    endif()

    # `--artifact-dir` is cargo's OWN copy, so nothing races it: it happens
    # inside the cargo run rather than in a later command that another leaf's
    # cargo could get between. It is unstable, which costs nothing here — this
    # crate is already pinned to a nightly and already passes `-Z build-std`.
    # It does NOT copy the depfile (verified), hence the explicit copy below.
    set(_artifact_dir_arg "")
    set(_depfile_retarget_cmd "")
    if(_artifact_dir)
        set(_artifact_dir_arg -Z unstable-options --artifact-dir "${_artifact_dir}")
        # issue 0820, second round — a COPY is not enough, and the difference is
        # invisible: a depfile names the artifact it describes, and ninja
        # CHECKS that name against the edge's output. Cargo names its OWN output
        # in the shared target dir; this command's OUTPUT is the artifact-dir
        # copy. Measured with ninja 1.13.2 on the real riscv leaf's depfile:
        #
        #   ninja explain: expected depfile 'real.d' to mention
        #     'nros-nuttx-ffi-out/nros-nuttx-ffi', got '<shared>/…/nros-nuttx-ffi'
        #
        # and on a mismatch ninja discards the depfile ENTIRELY and marks the
        # edge permanently dirty. So between 0805 and here, all 308 nano-ros
        # Rust paths were read by nobody and cargo re-ran on every build — the
        # always-run cost 0820 rejected, buying the correctness by accident
        # instead of by the edge. Retarget the rule, don't copy it.
        set(_depfile_retarget_cmd COMMAND ${CMAKE_COMMAND}
            "-DNROS_DEPFILE_IN=${_cargo_out_dir}/nros-nuttx-ffi.d"
            "-DNROS_DEPFILE_OUT=${_output_binary}.d"
            "-DNROS_DEPFILE_TARGET=${_output_binary}"
            -P "${_NROS_NUTTX_CMAKE_DIR}/nros-nuttx-depfile.cmake")
    endif()

    # 194.4: self-provision the NuttX export before the example links it. The
    # shared script (scripts/nuttx/build-nuttx.sh via NROS_NUTTX_PROVISION_SCRIPT)
    # is idempotent — its `.nros-nuttx-build-head` marker self-guards, so this is a
    # fast no-op once built. Runs in NUTTX_DIR with the board's NUTTX_* env (incl.
    # NUTTX_DEFCONFIG, the board's defconfig). The export self-provisions under
    # cmake / `nros build` — no separate kernel pre-build step.
    set(_provision_cmd "")
    if(NROS_NUTTX_PROVISION_SCRIPT AND EXISTS "${NROS_NUTTX_PROVISION_SCRIPT}")
        # Pass NUTTX_DIR + NUTTX_APPS_DIR explicitly so build-nuttx.sh never
        # falls to its PROJECT_ROOT default (which is wrong when the script is
        # invoked by absolute path from cmake). NUTTX_APPS_DIR may not have
        # reached this function's scope (set via nros_nuttx_validate PARENT_SCOPE
        # in the caller) — derive the repo-convention sibling from NUTTX_DIR (a
        # -D cache var, always visible) as a fallback.
        set(_nnbe_apps_dir "${NUTTX_APPS_DIR}")
        if(NOT _nnbe_apps_dir)
            get_filename_component(_nnbe_nuttx_parent "${NUTTX_DIR}" DIRECTORY)
            set(_nnbe_apps_dir "${_nnbe_nuttx_parent}/nuttx-apps")
        endif()
        # The script no longer derives the board defconfig from its own location
        # (it lives in shared scripts/nuttx/) — pass the board's defconfig through
        # NUTTX_DEFCONFIG when the overlay supplied one.
        set(_nnbe_defconfig_env "")
        if(NROS_NUTTX_DEFCONFIG)
            set(_nnbe_defconfig_env "NUTTX_DEFCONFIG=${NROS_NUTTX_DEFCONFIG}")
        endif()
        # 194.3c.3 — a new-arch board's Make.defs lives at a per-arch path
        # (boards/<arch>/<chip>/<board>/scripts/Make.defs); forward it through
        # NUTTX_BOARD_MAKEDEFS when the overlay supplied one (default in
        # build-nuttx.sh is the qemu-arm board, so arm overlays need not set it).
        set(_nnbe_makedefs_env "")
        if(NROS_NUTTX_BOARD_MAKEDEFS)
            set(_nnbe_makedefs_env "NUTTX_BOARD_MAKEDEFS=${NROS_NUTTX_BOARD_MAKEDEFS}")
        endif()
        set(_provision_cmd
            COMMAND ${CMAKE_COMMAND} -E env
                "NUTTX_DIR=${NUTTX_DIR}" "NUTTX_APPS_DIR=${_nnbe_apps_dir}"
                ${_nnbe_defconfig_env}
                ${_nnbe_makedefs_env}
                ${CMAKE_COMMAND} -E chdir "${NUTTX_DIR}"
                bash "${NROS_NUTTX_PROVISION_SCRIPT}")
    endif()

    add_custom_command(
        OUTPUT "${_output_binary}"
        ${_provision_cmd}
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${_NNBE_MAIN_SOURCE}"
            "APP_INCLUDE_DIRS_FILE=${_includes_file}"
            "APP_FFI_LIBS_FILE=${_ffi_libs_file}"
            "APP_EXTRA_SOURCES=${_extra_sources_str}"
            "APP_EXTRA_SOURCE_PKGS=${_source_pkgs_str}"
            "APP_INTERFACE_SOURCES=${_iface_sources_str}"
            "APP_COMPILE_DEFS=${_compile_defs_str}"
            "NUTTX_DIR=${NUTTX_DIR}"
            "NUTTX_APPS_DIR=${NUTTX_APPS_DIR}"
            "CARGO_TARGET_DIR=${_cargo_target_dir}"
            cargo build --profile ${_NROS_NUTTX_PROFILE} ${_artifact_dir_arg}
        ${_depfile_retarget_cmd}
        # Issue 0159 — make `cmake --build` itself honest: an exit-0 build with
        # no kernel ELF (up-to-date skip edge / a sub-step whose failure isn't
        # propagated) must fail HERE, not only in the outer fixture script's
        # artifact check (workspace-fixtures-build.sh backstop).
        COMMAND bash -c "test -f '${_output_binary}' || { echo 'nros: NuttX cross-link produced no kernel ELF: ${_output_binary}' >&2; exit 1; }"
        WORKING_DIRECTORY "${_NNBE_FFI_CRATE_DIR}"
        DEPENDS "${_NNBE_MAIN_SOURCE}" ${_NNBE_SOURCES} ${_NNBE_INTERFACE_SOURCES}
                "${_includes_file}" "${_ffi_libs_file}"
                "${_NNBE_FFI_CRATE_DIR}/build.rs"
                "${_NNBE_FFI_CRATE_DIR}/Cargo.toml"
        # issue 0820 — the DEPENDS list above names the app's C sources and this
        # crate's manifest, and NOTHING from the nano-ros Rust world. An edit to
        # nros-node or a backend left this command up to date, so cargo never
        # ran and the ELF kept the previous build's Rust code with a fresh mtime
        # — a museum binary (issue 0475's class), which cost a tier-2 test 90 s
        # and a long investigation before `rm -rf` "fixed" it.
        #
        # Hand-listing the Rust closure here would be a maintained
        # approximation of a graph cargo already computes exactly. It writes
        # that graph to `<output>.d` (Makefile format, absolute paths, 159
        # sources for this leaf), so consume it. Ninja + cmake >= 3.20 support
        # DEPFILE on add_custom_command; a missing depfile on the first build is
        # tolerated.
        DEPFILE "${_output_binary}.d"
        COMMENT "Building NuttX example: ${_NNBE_NAME}"
        VERBATIM)

    add_custom_target(${_NNBE_NAME}_build ALL DEPENDS "${_output_binary}")

    # Pull in each interface library's transitive dependency closure:
    # the cmake link graph already records `<pkg>__nano_ros_cpp` →
    # transitive `_gen` codegen targets, so a single add_dependencies
    # on each leaf interface lib chains the whole codegen DAG before
    # cargo runs.
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        add_dependencies(${_NNBE_NAME}_build ${_lib})
    endforeach()

    # Phase 156 (F3) — depend on corrosion's cross-built nros-c /
    # nros-cpp targets so their `build.rs` POST_BUILD mirror of
    # `nros_{,cpp_}config_generated.h` into the per-build
    # `<build_dir>/nano_ros/packages/core/nros-{c,cpp}/include/nros/`
    # dir completes BEFORE this app's nros-nuttx-ffi cargo build
    # runs. Without the dep the cargo build races corrosion + main.cpp
    # compile picks the source-tree `#error` stub.
    foreach(_dep cargo-build_nros_c cargo-build_nros_cpp)
        if(TARGET ${_dep})
            add_dependencies(${_NNBE_NAME}_build ${_dep})
        endif()
    endforeach()

    # Copy the kernel ELF to `<build>/<name>` — the path the tests resolve.
    #
    # issue 0882 — `BYPRODUCTS` is not cosmetic here. Without it this POST_BUILD
    # writes `<build>/<name>` as an UNDECLARED output: ninja attributes that path
    # to the only rule that claims it, which is the carrier `add_executable`, and
    # the carrier CANNOT link (see the board module — it is a property sink built
    # with the host toolchain). Asking for the carrier by name therefore ran a
    # failing link whose output ninja then DELETED, destroying this kernel.
    # Declaring the byproduct tells ninja who really produces the file, and pairs
    # with the carrier's `OUTPUT_NAME` so the two no longer write one path.
    add_custom_command(
        TARGET ${_NNBE_NAME}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy
            "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}"
        BYPRODUCTS "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}"
        COMMENT "Copying ${_NNBE_NAME} to build directory")
endfunction()

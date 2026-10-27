# cmake/NanoRosRuntimeCrate.cmake — Phase 241 W11 (Option D)
#
# Per-configure runtime umbrella for workspaces that contain a Rust Node pkg.
#
# Single-runtime invariant: a binary links exactly ONE Rust staticlib (one std, one
# nros-rmw-cffi C ABI + REGISTRY). A Rust Node pkg compiled to its own staticlib violates
# that — it re-bundles the full `nros` closure, colliding with the umbrella under GNU-ld
# once `--allow-multiple-definition` is gone (and splitting the stateful REGISTRY). The
# fix (Option D): bundle every workspace Rust node as a cargo **rlib** inside a synthesised
# staticlib that ALSO bundles `nros-cpp`, and make THAT staticlib the umbrella archive.
#
# Granularity is per cmake-configure (== per-arch: one configure is single-toolchain /
# single-`NROS_PLATFORM`; multi-arch workspaces bake one `build/<board>/` tree per
# board). So one runtime crate per configure serves every entry in that configure.
#
# `nros_synth_runtime_umbrella(BACKEND <b> PLATFORM <p>)` is called by
# `nano_ros_workspace` AFTER the SUBDIRS loop (so `nros-metadata.json` lists every Node
# pkg). It is a no-op when the configure has no Rust node — pure-C / pure-C++ workspaces
# keep `nros-cpp-headers` pointed at the plain `nros_cpp-static`.

if(DEFINED _NROS_RUNTIME_CRATE_INCLUDED)
    return()
endif()
set(_NROS_RUNTIME_CRATE_INCLUDED TRUE)

# Phase 241 W13/R1 — the BACKEND → {rlib, needs-cxx} dispatch (its cffi feature went in phase-506 W4d), which
# replaced a hardcoded `_nros_runtime_backend_feature` map here. phase-439 W4: it is
# no longer GENERATED from `resolve_rmw()`; `nros_rmw_dispatch()` asks the CLI, which
# reads the backend's own `nros-rmw.toml` over the provider scan (RFC-0094 D5). The
# synthesized runtime crate reaches it through `nros_feature_set` below rather than
# calling it directly.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRmwDispatch.cmake")
# phase-336 — the cargo-profile resolver. File scope, so the function below
# does not include() inside its own frame.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCargoProfile.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardFacts.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRosEdition.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")  # nros_codegen_tool_reconfigure (issue 1018)

# Map PLATFORM -> (nros-cpp platform feature ; std|alloc tier).
#
# phase-263 C2c — the umbrella now also serves GENUINELY-embedded cross targets (FreeRTOS
# thumbv7m, …), not just hosted workspaces. On a cross build Corrosion already cross-compiles
# the umbrella for the board triple (the board overlay set it), but the Rust TIER must match
# what the board builds plain nros-cpp with: `alloc;panic-halt` (NO `std` — thumbv7m et al.
# have no std; nros-serdes/nros-params pull `extern crate std` only under the std feature).
# Hosted posix builds keep the `std` host staticlib; threadx-linux and freertos-posix are
# `alloc` (issues 1763, 1778), like every ThreadX and FreeRTOS board.
# phase-314 — `_nros_runtime_platform_features` is GONE. It was the weaker of
# the two platform mappings: no BOARD input, so it could not split threadx-linux
# (std) from rv-virt-threadx (no_std). `nros_feature_set` in
# cmake/NanoRosFeatureSet.cmake is the one computation now.

# Write the synthesised `nros_ws_runtime` crate (Cargo.toml + src/lib.rs) into OUT_DIR.
# Factored out of nros_synth_runtime_umbrella (phase-263 C2c-zephyr) so BOTH the cmake/
# Corrosion lane AND the Zephyr/west lane (zephyr/CMakeLists.txt) synthesise the IDENTICAL
# umbrella crate (nros-cpp + every workspace Rust node bundled into one staticlib).
# CPP_FEATURES is an EXPLICIT arg (not derived from CMAKE_CROSSCOMPILING here).
# The root is always `#![no_std]` (phase-359 W10 stage C): there used to be a
# NO_STD arg because native_sim and posix took a `std` tier, and no tier does now.
#   OUT_DIR       — crate dir to write (Cargo.toml + src/lib.rs).
#   BACKEND       — zenoh|xrce|cyclone (selects the backend force-link anchor).
#   NODE_DIRS     — list of workspace Rust node pkg dirs (each with a Cargo.toml).
#   CPP_FEATURES  — the nros-cpp feature list baked into the umbrella's dep line.
# Issue 2085 — the central `[patch.crates-io]` table, for a generated root
# whose members name nano-ros crates by version (`nros = { version = "*" }`,
# every example since 2085). This umbrella is its own workspace root, built in
# a build dir that no `nros sync`-written `.cargo/config.toml` covers, so
# without the table cargo looks the member's `nros` up on crates.io and fails
# "no matching package named `nros` found". The table comes from the ONE
# writer `nros sync` uses (`nros ws central-patch`), never restated here, so the
# two cannot resolve against different crates.
function(_nros_central_patch_table out)
    set(_file "${NROS_REPO_DIR}/nros-patch.toml")
    if(_NANO_ROS_CODEGEN_TOOL)
        # Issue 1018 — a configure-time emitter carries its tool as a configure
        # dependency, so rebuilding `nros` re-runs this.
        nros_codegen_tool_reconfigure("${_NANO_ROS_CODEGEN_TOOL}")
        execute_process(
            COMMAND "${_NANO_ROS_CODEGEN_TOOL}" ws central-patch
                    --nano-ros-path "${NROS_REPO_DIR}"
            RESULT_VARIABLE _rc
            OUTPUT_QUIET
            ERROR_VARIABLE _err)
        if(NOT _rc EQUAL 0)
            message(FATAL_ERROR
                "nano-ros: `nros ws central-patch` failed (${_rc}); the runtime "
                "umbrella needs its [patch.crates-io] table (issue 2085).\n${_err}")
        endif()
    endif()
    if(EXISTS "${_file}")
        file(READ "${_file}" _table)
        set(${out} "${_table}" PARENT_SCOPE)
    else()
        message(STATUS
            "nano-ros: no ${_file} and no `nros` to write it — the runtime umbrella "
            "carries no [patch.crates-io]. Run `nros sync`.")
        nros_message_detail(
            "nano-ros: a workspace member that names a nano-ros crate by version "
            "(`nros = { version = \"*\" }`) resolves only through that table, so "
            "without it cargo looks the crate up on crates.io (issue 2085).")
        set(${out} "" PARENT_SCOPE)
    endif()
endfunction()

function(nros_write_runtime_umbrella_crate)
    cmake_parse_arguments(_W "" "OUT_DIR;BACKEND" "NODE_DIRS;CPP_FEATURES" ${ARGN})

    # ---- Per-node cargo path-deps + register-symbol anchors ----
    set(_dep_lines "")
    set(_anchor_lines "")
    foreach(_dir IN LISTS _W_NODE_DIRS)
        if(NOT EXISTS "${_dir}/Cargo.toml")
            message(FATAL_ERROR
                "nros_write_runtime_umbrella_crate: Rust node at '${_dir}' has no Cargo.toml.")
        endif()
        # Cargo package name (the dep key) from `name = "..."`.
        file(STRINGS "${_dir}/Cargo.toml" _name_line REGEX "^[ \t]*name[ \t]*=")
        list(GET _name_line 0 _name_line)
        string(REGEX REPLACE "^[ \t]*name[ \t]*=[ \t]*\"([^\"]+)\".*" "\\1" _crate "${_name_line}")
        # Register symbol uses the symbol-sanitised crate name (same transform as the
        # `nros::node!()` macro: non-alnum/underscore -> '_').
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _sym "${_crate}")
        string(APPEND _dep_lines
            "${_crate} = { path = \"${_dir}\" }\n")
        # Reference the node's typed install fn by its RUST CRATE PATH (not an `extern \"C\"`
        # import). The `nros::node!()` macro emits it as `#[no_mangle] pub extern \"C\" fn
        # __nros_component_<sym>_install(node, executor, self)` (phase-257 W0-B — the uniform
        # cross-language component-install seam); an extern import would only add an undefined
        # ref (the node rlib's object is never pulled), whereas naming the crate item forces
        # its codegen unit — incl. the no_mangle symbol — into this staticlib root. `<sym>` is
        # the dash→underscore crate ident, identical to the macro's symbol sanitisation.
        # (Phase-258 Track 1 — anchors `_install`, not the retired `_register` seam.)
        string(APPEND _anchor_lines
            "#[used]\n"
            "static _KEEP_NODE_${_sym}: unsafe extern \"C\" fn(*const core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void) -> i32 =\n"
            "    ${_sym}::__nros_component_${_sym}_install;\n")
    endforeach()

    # ---- Backend force-link anchor (zenoh / xrce bundle a Rust rlib backend; cyclone is a
    #      separate C++ lib with no Rust closure, so skip it) ----
    # RFC-0071 D9 (phase-506 W4a) — the backend reaches this umbrella the way it
    # reaches an image crate: as its own dependency, with its registration
    # anchored here, never as an `nros-cpp` feature (that branch used to name
    # zenoh and xrce). `nros_image_backend` is the one place that splits the
    # features and spells the dependency.
    include("${NROS_REPO_DIR}/cmake/NanoRosImageCrate.cmake")
    nros_image_backend(_ib RMW "${_W_BACKEND}" API_FEATURES ${_W_CPP_FEATURES})
    set(_backend_ctor "")
    set(_backend_dep_line "")
    set(_cpp_feats ${_W_CPP_FEATURES})
    if(_ib_BUNDLED)
        set(_cpp_feats ${_ib_API_FEATURES})
        set(_backend_dep_line "${_ib_DEP_LINE}\n")
        set(_backend_ctor "${_ib_ANCHOR}")
    endif()
    # This umbrella is its own workspace root, so it builds with only the
    # profiles it states. The `nros-*` presets also arrive as `CARGO_PROFILE_*`
    # (phase-336), but `release` and `dev` never do, so without these a
    # `--profile release` build got cargo's `panic = "unwind"` (issue 1953).
    # Copied from the root manifest, not restated.
    nros_root_cargo_profiles(_profiles REPO "${NROS_REPO_DIR}")
    _nros_central_patch_table(_patch)
    set(_feat_toml "")
    foreach(_f IN LISTS _cpp_feats)
        if(NOT _f MATCHES "/")
            string(APPEND _feat_toml "\"${_f}\", ")
        endif()
    endforeach()
    file(WRITE "${_W_OUT_DIR}/Cargo.toml"
"# Generated by nano_ros_workspace (NanoRosRuntimeCrate.cmake) — DO NOT EDIT.
# Phase 241 W11 (Option D) per-configure runtime umbrella: nros-cpp + every workspace
# Rust node, bundled into ONE staticlib so the binary links a single Rust runtime.
[package]
name = \"nros_ws_runtime\"
version = \"0.0.0\"
edition = \"2024\"
publish = false

[lib]
path = \"src/lib.rs\"
crate-type = [\"staticlib\"]

[dependencies]
nros-cpp = { path = \"${NROS_REPO_DIR}/packages/api/nros-cpp\", default-features = false, features = [${_feat_toml}] }
${_backend_dep_line}${_dep_lines}
[workspace]
${_profiles}
${_patch}")

    # phase-263 C2c — the umbrella crate itself must be no_std (else IT pulls `std`, and
    # libstd's `panic_impl` meets nros-c's `panic-platform` handler as E0152). The anchor
    # surface below is all `extern "C"` fns + `core::` slices — no_std-clean. Every tier,
    # hosted and native_sim included, since phase-359 W10 stage C.
    set(_no_std_attr "#![no_std]\n")

    file(WRITE "${_W_OUT_DIR}/src/lib.rs"
"// Generated by nano_ros_workspace (NanoRosRuntimeCrate.cmake) — DO NOT EDIT.
// Phase 241 W11 (Option D) per-configure runtime umbrella.
${_no_std_attr}#![allow(unused)]

// Re-pull nros-cpp's full ABI surface (nros-c C API + nros-cpp C++ FFI + backend register
// closure) past staticlib DCE — nros-cpp is a dependency rlib here, so its own #[used]
// anchors are dropped before this staticlib root is emitted.
#[used]
static _KEEP_SURFACE: &[&[unsafe extern \"C\" fn()]] = nros_cpp::FORCE_LINK_ANCHOR;

${_backend_ctor}
// One anchor per workspace Rust node — keep its __nros_component_<pkg>_register C symbol
// (called by the entry's generated `main`) from staticlib DCE.
${_anchor_lines}")
endfunction()

function(nros_synth_runtime_umbrella)
    cmake_parse_arguments(_NRR "" "BACKEND;PLATFORM;EDITION" "" ${ARGN})
    _nros_resolve_ros_edition("${_NRR_EDITION}" _NRR_EDITION)
    if(NOT _NRR_BACKEND)
        set(_NRR_BACKEND zenoh)
    endif()
    if(NOT _NRR_PLATFORM)
        set(_NRR_PLATFORM posix)
    endif()

    set(_meta "${CMAKE_BINARY_DIR}/nros-metadata.json")
    if(NOT EXISTS "${_meta}")
        return()  # no node_register ran — nothing to do
    endif()
    file(READ "${_meta}" _json)

    # Collect Rust node pkg dirs from the components array.
    string(JSON _ncomp ERROR_VARIABLE _err LENGTH "${_json}" components)
    if(_err OR NOT _ncomp)
        return()
    endif()
    set(_rust_dirs "")
    math(EXPR _last "${_ncomp} - 1")
    foreach(_i RANGE 0 ${_last})
        string(JSON _lang ERROR_VARIABLE _e1 GET "${_json}" components ${_i} lang)
        if(_lang STREQUAL "rust")
            string(JSON _dir ERROR_VARIABLE _e2 GET "${_json}" components ${_i} pkg_dir)
            if(NOT _e2)
                list(APPEND _rust_dirs "${_dir}")
            endif()
        endif()
    endforeach()
    list(REMOVE_DUPLICATES _rust_dirs)
    if(NOT _rust_dirs)
        return()  # pure-C / pure-C++ workspace — keep nros_cpp-static as the umbrella
    endif()

    if(NOT COMMAND corrosion_import_crate)
        message(FATAL_ERROR
            "nros_synth_runtime_umbrella: Corrosion required (build via "
            "nano_ros_workspace()/add_subdirectory(nano-ros)).")
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR "nros_synth_runtime_umbrella: NROS_REPO_DIR not set.")
    endif()

    # phase-314 — ONE feature computation, shared with nros-c and nros-cpp.
    # This site used to build its own list: it honoured the edition (correctly)
    # but had no BOARD input, so it could not split threadx-linux (std) from
    # rv-virt-threadx (no_std), and it carried no capabilities at all — which meant
    # a MIXED workspace silently lost param-services while a pure C/C++ one kept
    # them (issue 0311 / phase-314 W1).
    # phase-439 W4 — the bare `nros_rmw_dispatch("${_NRR_BACKEND}")` that stood
    # here is GONE. It read none of the eight variables it set: the next line
    # includes `NanoRosFeatureSet`, which computes the API features from
    # `_NRR_BACKEND` itself, so the call was a no-op that read as the SSoT
    # (`grep NROS_RMW` over this file returned nothing). It is a process spawn
    # now, which is a good reason to notice, but it was dead before.
    include("${NROS_REPO_DIR}/cmake/NanoRosFeatureSet.cmake")
    set(_caps ${NROS_FEATURES})
    if(NROS_SAFETY_E2E)
        list(APPEND _caps safety)
    endif()
    # phase-323 W2 — capabilities come from the DECLARATION on every platform.
    #
    # posix used to get `param_services` + `lifecycle` unconditionally. That was not
    # a convenience: until W1 it was the ONLY route those axes took on hosted, since
    # `NROS_FEATURES` was never populated on the workspace path (issue 0353). So
    # hosted could not fail when a declaration was missing, and the same
    # `system.toml` produced different runtimes depending on where it was built.
    #
    # W1 made `nano_ros_workspace()` resolve the axes from `SYSTEM` before the
    # import, so the declaration now arrives on its own and this can go.
    nros_feature_set(_cpp_features
        CRATE        cpp
        EDITION      "${_NRR_EDITION}"
        RMW          "${_NRR_BACKEND}"
        PLATFORM     "${_NRR_PLATFORM}"
        CAPABILITIES "${_caps}")

    # phase-366 — and the image's ending, which `nros_feature_set` cannot know.
    #
    # This runs AFTER the SUBDIRS loop (see nano_ros_workspace), so every
    # `nano_ros_entry(PANIC ...)` in this configure has already recorded its
    # policy, and the entry itself has already rejected the case where two of
    # them disagree. The entry applies the feature to nros_cpp-static with
    # `corrosion_set_features()`; here the archive is a DIFFERENT crate, whose
    # nros-cpp dependency is spelled `default-features = false` — so the feature
    # has to be baked into that dep line or nothing carries it, and the entry's
    # generated header meets an nros-cpp built without it as issue-0369's
    # variant-anchor undefined symbol.
    #
    # No entry in the configure means no image, so the crate's own default
    # (`panic-platform`) is the honest stand-in: same ending an entry that says
    # nothing gets.
    get_property(_umb_panic GLOBAL PROPERTY NROS_ENTRY_PANIC_POLICY)
    if(NOT _umb_panic)
        set(_umb_panic platform)
    endif()
    nros_panic_policy_feature(_umb_panic_feature
        "${_umb_panic}" "nros_synth_runtime_umbrella")
    if(_umb_panic_feature)
        list(APPEND _cpp_features ${_umb_panic_feature})
    endif()

    # Phase 241 W11 was inlined here; phase-263 C2c-zephyr factored it into
    # nros_write_runtime_umbrella_crate so the Zephyr/west lane reuses the IDENTICAL synthesis.
    # The cmake/Corrosion lane keeps no_std == CMAKE_CROSSCOMPILING (its embedded targets are
    # genuinely-no_std cross — FreeRTOS thumbv7m et al.).
    set(_crate_dir "${CMAKE_BINARY_DIR}/nros_ws_runtime")
    #
    # issue 1763 — …OR when the runtime it bundles has no `std`. Being cross was
    # only a stand-in for "this tier has no std", and threadx-linux breaks the
    # stand-in: a HOST build whose `nros-cpp` is `alloc` (the ThreadX tier). An
    # umbrella root without `#![no_std]` links libstd implicitly, and libstd's
    # `panic_impl` then meets `nros-c`'s `panic-platform` handler as E0152
    # (measured on `workspaces/mixed`, image `threadx`). The feature list is the
    # fact that decides it, and the line above just computed it. Cross stays
    # sufficient on its own, so that no existing cross image changes shape
    # (NuttX's `nros-cpp` carries `std` and its umbrella has always been
    # `#![no_std]`).
    #
    # phase-359 W10 stage C — and now always: `nros-cpp` has no `std`, so no
    # runtime it bundles has one, posix included.
    nros_write_runtime_umbrella_crate(
        OUT_DIR      "${_crate_dir}"
        BACKEND      "${_NRR_BACKEND}"
        NODE_DIRS    ${_rust_dirs}
        CPP_FEATURES ${_cpp_features})

    # The runtime crate has no features of its own — the backend/platform/ros selection is
    # baked into the `nros-cpp` dependency's `features = [...]` line above (with
    # `default-features = false`). So import with no FEATURES; passing nros-cpp's features
    # to this package fails ("does not contain these features").
    # phase-336 — the profile is passed, not left to Corrosion's build-type
    # default, and its definition is injected so the SYNTHESIZED manifest needs
    # no `[profile.*]` block of its own (this file used to mirror one).
    nros_resolve_cargo_profile()
    corrosion_import_crate(
        MANIFEST_PATH "${_crate_dir}/Cargo.toml"
        CRATES        nros_ws_runtime
        CRATE_TYPES   staticlib
        PROFILE       ${NROS_CARGO_PROFILE}
    )
    nros_cargo_profile_env(nros_ws_runtime-static)
# issue 0657 — the riscv64 float ABI, per target (see the helper's comment).
nros_riscv64_rustflags_env(nros_ws_runtime-static)
# phase-372 W1 — the cortex-r52 FPU cflags, same class one arch over.
nros_armv8r_cflags_env(nros_ws_runtime-static)
    # phase-351 W5 — the board rung + site config reach cargo HERE, because a
    # workspace member's own `.cargo/config.toml` never does (corrosion invokes
    # cargo from the workspace root).
    nros_board_facts_env(nros_ws_runtime-static)
    # phase-392 W5.c — and the entity figures the RMW sizes its queryable table
    # from.
    #
    # DEFERRED to the end of the TOP-LEVEL scope (phase-392 W5.g), because the
    # comment this replaced was wrong about the ordering and the mechanism has
    # therefore never run anywhere. It said the accumulation "is complete only
    # after the SUBDIRS loop above has processed them all" — true of the NODE
    # packages, false of the ENTRY, which `nano_ros_add_entry` declares LAST by
    # design ("the first point in a configure that is guaranteed to be AFTER
    # every nano_ros_node_register()"). `nros_record_entity_facts` ran there
    # (retired, phase-486); the sizing descriptor's inputs are recorded there.
    #
    # MEASURED on `examples/workspaces/mixed`: the consumer logged
    # `seen=<empty>` at configure line 17 and the seven models were recorded at
    # lines 19-85. It read the accumulator before anything filled it, every
    # time, in every workspace — which is why W5.g found no status line in any
    # of the three it checked and could not explain `mixed`.
    #
    # Same fix and same reasoning as `_nano_ros_support_schedule_flush`: DEFER
    # to `CMAKE_SOURCE_DIR`, not to the current directory. Deferring to the
    # current one would fire at the end of whichever scope called first, which
    # is the bug rather than a smaller version of it.
    include("${NROS_REPO_DIR}/cmake/NanoRosEntityFacts.cmake")
    nros_entity_facts_env_deferred(nros_ws_runtime-static)
    if(NOT TARGET nros_ws_runtime-static)
        message(FATAL_ERROR
            "nros_synth_runtime_umbrella: Corrosion did not create "
            "nros_ws_runtime-static.")
    endif()

    # ---- Re-point the umbrella archive: nros-cpp-headers (== NanoRos::NanoRosCpp) links
    #      the runtime staticlib instead of nros_cpp-static. All other INTERFACE wiring
    #      (includes, cyclone, stdc++) is preserved; nros_cpp-static stays built but
    #      unreferenced. ----
    if(TARGET nros-cpp-headers)
        get_target_property(_links nros-cpp-headers INTERFACE_LINK_LIBRARIES)
        if(_links)
            list(TRANSFORM _links REPLACE "^nros_cpp-static$" "nros_ws_runtime-static")
            set_target_properties(nros-cpp-headers PROPERTIES
                INTERFACE_LINK_LIBRARIES "${_links}")
        else()
            target_link_libraries(nros-cpp-headers INTERFACE nros_ws_runtime-static)
        endif()
        message(STATUS
            "nano-ros: workspace has Rust node(s) — umbrella archive => "
            "nros_ws_runtime (nros-cpp + ${_rust_dirs}).")
    endif()

    # ---- Issue #57 — mixed (C + C++ + Rust) workspaces ALSO link the C umbrella
    #      (`NanoRos::NanoRos` == the `NanoRos` INTERFACE target == `nros_c-static`),
    #      because the C Node pkgs link it. The runtime staticlib already bundles
    #      nros-c (via nros-cpp's dep) and re-pulls its full C ABI past DCE (the
    #      anchor above), so leaving `nros_c-static` separately linked double-defines
    #      nros-rmw-cffi's `REGISTRY` / `nros_rmw_cffi_*` / `nros_rmw_<x>_register`
    #      (link-time `multiple definition`). Repoint the C umbrella at the runtime
    #      staticlib too, mirroring the nros-cpp-headers swap, so nros-c resolves
    #      from the single umbrella archive. No-op for pure-C++/Rust workspaces (no
    #      `nros_c-static` in the interface list). ----
    if(TARGET NanoRos)
        get_target_property(_clinks NanoRos INTERFACE_LINK_LIBRARIES)
        if(_clinks AND ("nros_c-static" IN_LIST _clinks))
            # Preserve nros_c-static's INTERFACE include dirs (the per-build variant
            # header path + public `include/`, carrying `rclc/types.h` etc.) directly
            # on NanoRos BEFORE dropping it from the link list — the runtime staticlib
            # does not re-export them, and the C Node interface headers `#include` them.
            if(TARGET nros_c-static)
                get_target_property(_cinc nros_c-static INTERFACE_INCLUDE_DIRECTORIES)
                if(_cinc)
                    target_include_directories(NanoRos INTERFACE ${_cinc})
                endif()
            endif()
            list(TRANSFORM _clinks REPLACE "^nros_c-static$" "nros_ws_runtime-static")
            set_target_properties(NanoRos PROPERTIES
                INTERFACE_LINK_LIBRARIES "${_clinks}")
        endif()
    endif()

    # The per-build variant headers (`nros_cpp_config_generated.h` / `nros_config_generated.h`,
    # the *_OPAQUE_U64S / *_SIZE macros a C++ entry TU needs) are mirrored into
    # nros-cpp-headers' INTERFACE include dir by `cargo-build_nros_cpp`'s POST_BUILD
    # (nros-cpp/CMakeLists.txt). With the archive swap nothing links `nros_cpp-static`, so
    # that target would never build and the mirror would never run — the source-tree stub
    # (`#error … supplied per-build`) would win. Keep it in the build graph as an ordered
    # prerequisite of the runtime crate; its features match nros_ws_runtime's nros-cpp, so
    # the mirrored sizes are identical. The archive itself stays unlinked (harmless).
    if(TARGET cargo-build_nros_ws_runtime AND TARGET cargo-build_nros_cpp)
        add_dependencies(cargo-build_nros_ws_runtime cargo-build_nros_cpp)
    endif()

    # phase-263 A1 (mixed) — the SAME for the nros-C per-build header. A C Node pkg in a MIXED
    # workspace compiles a C component that `#include <rclc/detail/nros_generated.h>` →
    # `<rclc/detail/nros_config_generated.h>` (the *_OPAQUE_U64S / *_SIZE macros). Unlike nros-cpp's
    # POST_BUILD mirror, nros-c mirrors via a SEPARATE custom target `nros_c_config_header`
    # (Issue 0088 — a real Ninja edge, not a POST_BUILD side effect). With the umbrella archive
    # swap nothing links `nros_c-static`, so that mirror target is never pulled into the build and
    # the C component reads the source-tree stub (`SESSION_OPAQUE_U64S undeclared`). This was
    # latent — exposed only on a FULL clean rebuild (C component objects are otherwise
    # inputsig-cached). Keep the mirror target in the graph so the C header populates before the C
    # components compile. (Targets `nros_c_config_header`, the COPY — not `cargo-build_nros_c`,
    # which only writes the byproduct into the cargo dir, not the mirror.)
    if(TARGET cargo-build_nros_ws_runtime AND TARGET nros_c_config_header)
        add_dependencies(cargo-build_nros_ws_runtime nros_c_config_header)
    endif()
    # nros-cpp ALSO exposes a real-edge mirror custom target (`nros_cpp_config_header`); the
    # cargo-build_nros_cpp dep above only orders against the POST_BUILD (no edge → a parallel
    # clean build races the cpp component ahead of the mirror copy → `NROS_GUARD_CONDITION_SIZE`
    # / `nros_cpp_config_generated.h` stub). Pull the real mirror target into the graph too.
    if(TARGET cargo-build_nros_ws_runtime AND TARGET nros_cpp_config_header)
        add_dependencies(cargo-build_nros_ws_runtime nros_cpp_config_header)
    endif()
endfunction()

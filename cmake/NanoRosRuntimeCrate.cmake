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
# single-`NANO_ROS_PLATFORM`; multi-arch workspaces bake one `build/<board>/` tree per
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

# Phase 241 W13/R1 — the BACKEND → {cffi feature, rlib, extra link libs, needs-cxx}
# dispatch is GENERATED from cargo-nano-ros `resolve_rmw()` (the RFC-0031 SSoT) into
# `NanoRosRmwDispatch.cmake` (drift-guarded by `rmw_cmake_dispatch_is_current`). The
# former hardcoded `_nros_runtime_backend_feature` map is replaced by the generated
# `nros_rmw_dispatch(<rmw>)` so the synthesized runtime crate's cffi feature can never
# drift from the Rust SSoT / the cmake link extras.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRmwDispatch.cmake")

# Map PLATFORM -> (nros-cpp platform feature ; std|alloc tier). Hosted workspaces only —
# the runtime umbrella is a host/native-style staticlib (Corrosion). Embedded boards bake
# their own trees and link the umbrella through the board path.
function(_nros_runtime_platform_features platform out_feats)
    if(platform STREQUAL "posix")
        set(${out_feats} "std;platform-posix" PARENT_SCOPE)
    else()
        # Other hosted platforms (threadx_linux, …) still build a host staticlib; default
        # to std + the matching platform feature. Extend here as cells are added.
        set(${out_feats} "std;platform-${platform}" PARENT_SCOPE)
    endif()
endfunction()

function(nros_synth_runtime_umbrella)
    cmake_parse_arguments(_NRR "" "BACKEND;PLATFORM" "" ${ARGN})
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
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR "nros_synth_runtime_umbrella: NANO_ROS_ROOT not set.")
    endif()

    # W13/R1 — pull the cffi feature from the generated dispatch (SSoT: resolve_rmw).
    nros_rmw_dispatch("${_NRR_BACKEND}")
    set(_backend_feat "${NROS_RMW_UMBRELLA_CFFI_FEATURE}")
    _nros_runtime_platform_features("${_NRR_PLATFORM}" _plat_feats)
    set(_cpp_features "ros-humble" "${_backend_feat}" ${_plat_feats})

    # ---- Per-node cargo path-deps + register-symbol anchors ----
    set(_dep_lines "")
    set(_anchor_lines "")
    foreach(_dir IN LISTS _rust_dirs)
        if(NOT EXISTS "${_dir}/Cargo.toml")
            message(FATAL_ERROR
                "nros_synth_runtime_umbrella: Rust node at '${_dir}' has no Cargo.toml.")
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
        # Reference the node's register fn by its RUST CRATE PATH (not an `extern \"C\"`
        # import). The `nros::node!()` macro emits it as `#[no_mangle] pub extern \"C\" fn
        # __nros_component_<sym>_register`; an extern import would only add an undefined ref
        # (the node rlib's object is never pulled), whereas naming the crate item forces its
        # codegen unit — incl. the no_mangle symbol — into this staticlib root. `<sym>` is
        # the dash→underscore crate ident, identical to the macro's symbol sanitisation.
        string(APPEND _anchor_lines
            "#[used]\n"
            "static _KEEP_NODE_${_sym}: unsafe extern \"C\" fn(*mut core::ffi::c_void) -> i32 =\n"
            "    ${_sym}::__nros_component_${_sym}_register;\n")
    endforeach()

    # ---- Backend force-link anchor (zenoh / xrce bundle a Rust rlib backend; cyclone is a
    #      separate C++ lib with no Rust closure, so skip it) ----
    # Phase 249 P3: this is a plain `#[used]` force-link anchor, NOT an `.init_array` ctor.
    # It keeps the backend closure (incl. the `nros_rmw_<x>_register` C export) linked into
    # the runtime staticlib past DCE; registration itself is the one explicit call — the
    # generated strong `nros_app_register_backends()` (P2b) invokes `nros_rmw_<x>_register`.
    set(_backend_ctor "")
    if(_NRR_BACKEND STREQUAL "zenoh" OR _NRR_BACKEND STREQUAL "xrce")
        set(_backend_ctor
"// Force-link the backend closure at the staticlib root (nros-cpp's own anchor is DCE'd
// as a dependency rlib). NOT a ctor — the generated nros_app_register_backends() strong
// def does the registration explicitly (phase-249 P3).
#[used]
static _KEEP_BACKEND: unsafe extern \"C\" fn() = nros_cpp::nros_cpp_auto_register_backend;
")
    endif()

    # ---- Synthesise the crate ----
    set(_crate_dir "${CMAKE_BINARY_DIR}/nros_ws_runtime")
    set(_feat_toml "")
    foreach(_f IN LISTS _cpp_features)
        string(APPEND _feat_toml "\"${_f}\", ")
    endforeach()

    file(WRITE "${_crate_dir}/Cargo.toml"
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
nros-cpp = { path = \"${NANO_ROS_ROOT}/packages/core/nros-cpp\", default-features = false, features = [${_feat_toml}] }
${_dep_lines}
[workspace]
")

    file(WRITE "${_crate_dir}/src/lib.rs"
"// Generated by nano_ros_workspace (NanoRosRuntimeCrate.cmake) — DO NOT EDIT.
// Phase 241 W11 (Option D) per-configure runtime umbrella.
#![allow(unused)]

// Re-pull nros-cpp's full ABI surface (nros-c C API + nros-cpp C++ FFI + backend register
// closure) past staticlib DCE — nros-cpp is a dependency rlib here, so its own #[used]
// anchors are dropped before this staticlib root is emitted.
#[used]
static _KEEP_SURFACE: &[&[unsafe extern \"C\" fn()]] = nros_cpp::FORCE_LINK_ANCHOR;

${_backend_ctor}
// One anchor per workspace Rust node — keep its __nros_component_<pkg>_register C symbol
// (called by the entry's generated `main`) from staticlib DCE.
${_anchor_lines}")

    # The runtime crate has no features of its own — the backend/platform/ros selection is
    # baked into the `nros-cpp` dependency's `features = [...]` line above (with
    # `default-features = false`). So import with no FEATURES; passing nros-cpp's features
    # to this package fails ("does not contain these features").
    corrosion_import_crate(
        MANIFEST_PATH "${_crate_dir}/Cargo.toml"
        CRATES        nros_ws_runtime
        CRATE_TYPES   staticlib
    )
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
endfunction()

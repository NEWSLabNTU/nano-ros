# cmake/NanoRosImageCrate.cmake — RFC-0071 D9 (phase-506 W4a)
#
# A C/C++ image's Rust staticlib has to CONTAIN its RMW backend: one staticlib,
# one cffi registry (phase-241 W11, issue 0436). Until W4a the API crates did
# that themselves, through a feature per backend (`nros-c/rmw-zenoh`,
# `nros-cpp/rmw-zenoh-cffi`, …), which is why `nros-c` and `nros-cpp` named
# backends and a third-party backend needed an edit to both (issue 1297).
# W4d deleted those features, so this crate is now the ONLY way a C/C++ image's
# staticlib carries a bundled (rlib) backend.
#
# This generates, per configure, the crate that does it instead: an "image
# crate" that depends on the API crate WITHOUT a backend feature, and on the
# backend crate the descriptor names (`[rmw.provides.cargo] register_crate` /
# `register_dir`), forwarding the image's platform, edition and capability
# features through the descriptor's own tables, and anchoring the backend's
# `register` so staticlib DCE keeps it.
#
# The image crate's LIB is named like the API crate it stands in for
# (`[lib] name = "nros_c"` / `"nros_cpp"`), and the API crate is a renamed
# dependency (`api = { package = "nros-c", … }`). Corrosion names its targets
# after the lib, so `nros_c-static`, `cargo-build_nros_c` and every consumer of
# those names are unchanged: only the manifest and the package move.
#
#   nros_image_crate(<prefix>
#       FLAVOR       c | cpp
#       RMW          <rmw>
#       OUT_DIR      <dir>
#       API_FEATURES <features nros_feature_set() derived for the API crate>
#       [ALWAYS])
#
# Sets in the caller's scope:
#   <prefix>_MANIFEST  the manifest to import
#   <prefix>_CRATE     the package to import
#   <prefix>_FEATURES  the features to import it with
#   <prefix>_LOCKED    LOCKED, or empty when the root has no seeded lock
#
# A backend the image does not bundle as an rlib (CMake-linked Cyclone and
# uORB, or no backend) leaves everything as it was: the API crate itself, with
# the features it was given — unless ALWAYS, for a road whose archive path
# follows the image root and so needs one root whatever the backend (the NuttX
# apps Makefile, below).
#
# Script mode, for a road with no CMake configure of its own (the NuttX apps
# Makefile) — the same generator, so the image crate has one spelling:
#
#   cmake -DNROS_IMAGE_FLAVOR=c|cpp -DNROS_IMAGE_RMW=<rmw>
#         -DNROS_IMAGE_EDITION=<edition> -DNROS_IMAGE_OUT_DIR=<dir>
#         [-DNROS_IMAGE_WORKSPACE_DIR=<dir>]
#         "-DNROS_IMAGE_API_FEATURES=<f;f;…>"
#         -P cmake/NanoRosImageCrate.cmake
#
# prints `<manifest> <feature,feature,…>` on stdout, and always generates.

include_guard(GLOBAL)
if(CMAKE_SCRIPT_MODE_FILE STREQUAL CMAKE_CURRENT_LIST_FILE)
    # A configure has its project's policies; `-P` starts with none, and the
    # included modules use `IN_LIST` (CMP0057).
    cmake_policy(VERSION 3.22)
endif()
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRmwDispatch.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRootProfiles.cmake")

set(_NROS_IMAGE_CRATE_REPO "${CMAKE_CURRENT_LIST_DIR}/.."
    CACHE INTERNAL "nano-ros checkout, for NanoRosImageCrate.cmake")

# `k=v;k=v` -> the value of key `k`, or "".
function(_nros_image_map_get out map key)
    set(_val "")
    foreach(_pair IN LISTS map)
        if(_pair MATCHES "^([^=]+)=(.*)$" AND CMAKE_MATCH_1 STREQUAL key)
            set(_val "${CMAKE_MATCH_2}")
            break()
        endif()
    endforeach()
    set(${out} "${_val}" PARENT_SCOPE)
endfunction()

# The backend half of an image, shared by the image crate and the Rust-node
# workspace umbrella (`NanoRosRuntimeCrate.cmake`): which of the API features
# stay on the API crate, the backend dependency line, and the Rust that anchors
# its registration.
#
#   nros_image_backend(<prefix> RMW <rmw> API_FEATURES <list>)
#
# Sets <prefix>_BUNDLED (TRUE when the backend is an rlib this image bundles),
# and when it is: <prefix>_API_FEATURES, <prefix>_DEP_LINE, <prefix>_ANCHOR.
function(nros_image_backend prefix)
    cmake_parse_arguments(_IB "" "RMW" "API_FEATURES" ${ARGN})
    if(_IB_RMW STREQUAL "" OR _IB_RMW STREQUAL "none")
        set(${prefix}_BUNDLED FALSE PARENT_SCOPE)
        return()
    endif()
    nros_rmw_dispatch("${_IB_RMW}")
    if(NOT NROS_RMW_LINK_STRATEGY STREQUAL "umbrella" OR NOT NROS_RMW_REGISTER_CRATE)
        set(${prefix}_BUNDLED FALSE PARENT_SCOPE)
        return()
    endif()

    # Split what nros_feature_set() derived: the API crate's own features stay
    # on the API crate, minus `<backend>/<feature>` rows (the edition's backend
    # destination, issue 1893), which become features of the backend dep.
    set(_api_feats "")
    set(_backend_feats "")
    foreach(_f IN LISTS _IB_API_FEATURES)
        if(_f MATCHES "^([^/]+)/(.+)$")
            list(APPEND _backend_feats "${CMAKE_MATCH_2}")
        else()
            list(APPEND _api_feats "${_f}")
        endif()
    endforeach()
    if(NOT "rmw-cffi" IN_LIST _api_feats)
        list(APPEND _api_feats rmw-cffi)
    endif()
    # The platform and the capabilities, through the backend's own tables.
    foreach(_f IN LISTS _api_feats)
        _nros_image_map_get(_pf "${NROS_RMW_PLATFORM_FEATURES}" "${_f}")
        if(_pf)
            list(APPEND _backend_feats "${_pf}")
        endif()
        foreach(_cap IN LISTS NROS_RMW_CAPABILITY_FEATURES)
            if(_cap MATCHES "^[^=]+=(.+)$" AND CMAKE_MATCH_1 STREQUAL _f)
                list(APPEND _backend_feats "${_f}")
            endif()
        endforeach()
    endforeach()
    list(REMOVE_DUPLICATES _backend_feats)
    set(_backend_toml "")
    foreach(_f IN LISTS _backend_feats)
        string(APPEND _backend_toml "\"${_f}\", ")
    endforeach()
    string(REPLACE "-" "_" _reg_ident "${NROS_RMW_REGISTER_CRATE}")
    set(${prefix}_BUNDLED TRUE PARENT_SCOPE)
    set(${prefix}_API_FEATURES "${_api_feats}" PARENT_SCOPE)
    set(${prefix}_DEP_LINE
        "${NROS_RMW_REGISTER_CRATE} = { path = \"${NROS_RMW_REGISTER_DIR}\", default-features = false, features = [${_backend_toml}] }"
        PARENT_SCOPE)
    set(${prefix}_ANCHOR
"// The backend's registration, anchored so staticlib DCE keeps it (the
// generated strong `nros_app_register_backends()` calls its C register symbol).
#[used]
static _KEEP_BACKEND: fn() = || {
    let _ = ::${_reg_ident}::${NROS_RMW_REGISTER_FN}();
};
" PARENT_SCOPE)
endfunction()

function(nros_image_crate prefix)
    cmake_parse_arguments(_IC "ALWAYS" "FLAVOR;RMW;OUT_DIR;WORKSPACE_DIR" "API_FEATURES" ${ARGN})
    if(_IC_FLAVOR STREQUAL "c")
        set(_api_pkg nros-c)
        set(_api_lib nros_c)
    elseif(_IC_FLAVOR STREQUAL "cpp")
        set(_api_pkg nros-cpp)
        set(_api_lib nros_cpp)
    else()
        message(FATAL_ERROR "nros_image_crate: FLAVOR must be c or cpp, got '${_IC_FLAVOR}'")
    endif()
    get_filename_component(_repo "${_NROS_IMAGE_CRATE_REPO}" ABSOLUTE)
    set(_api_dir "${_repo}/packages/api/${_api_pkg}")

    # The road as it was: the API crate, its own manifest, its own features.
    set(${prefix}_MANIFEST "${_api_dir}/Cargo.toml" PARENT_SCOPE)
    set(${prefix}_CRATE "${_api_pkg}" PARENT_SCOPE)
    set(${prefix}_FEATURES "${_IC_API_FEATURES}" PARENT_SCOPE)
    set(${prefix}_LOCKED LOCKED PARENT_SCOPE)

    nros_image_backend(_ib RMW "${_IC_RMW}" API_FEATURES ${_IC_API_FEATURES})
    if(_ib_BUNDLED)
        set(_api_feats ${_ib_API_FEATURES})
        nros_rmw_dispatch("${_IC_RMW}")  # memoised; brings NROS_RMW_* into this scope
        set(_bundles "${NROS_RMW_REGISTER_CRATE}")
    elseif(_IC_ALWAYS)
        set(_api_feats ${_IC_API_FEATURES})
        set(_ib_DEP_LINE "")
        set(_ib_ANCHOR "")
        set(_bundles "no backend")
    else()
        return()
    endif()

    # Every feature of the API crate, forwarded (`f = ["api/f"]`), so the
    # import keeps passing the API crate's features BY NAME: the panic-policy
    # feature is appended to the target after import, and sees the same names
    # as before.
    file(STRINGS "${_api_dir}/Cargo.toml" _api_lines)
    set(_in_features FALSE)
    set(_forward "")
    foreach(_line IN LISTS _api_lines)
        if(_line MATCHES "^\\[")
            if(_line MATCHES "^\\[features\\]")
                set(_in_features TRUE)
            else()
                set(_in_features FALSE)
            endif()
        elseif(_in_features AND _line MATCHES "^([A-Za-z0-9_-]+)[ \t]*=")
            set(_feat "${CMAKE_MATCH_1}")
            if(NOT _feat STREQUAL "default")
                string(APPEND _forward "${_feat} = [\"api/${_feat}\"]\n")
            endif()
        endif()
    endforeach()
    # phase-359 W10 stage C — every image root is `#![no_std]`: the API crates
    # have no `std` feature to infer a hosted root from, and the panic handler,
    # personality and allocator are the image's, supplied by the API crate's
    # `panic-*` / `global-allocator` features this root forwards.
    set(_no_std "#![no_std]\n")
    if(_IC_FLAVOR STREQUAL "c")
        set(_surface "static _KEEP_API: &[unsafe extern \"C\" fn()] = &api::c_surface_anchor::C_SURFACE_ANCHOR;")
    else()
        set(_surface "static _KEEP_API: &[&[unsafe extern \"C\" fn()]] = api::FORCE_LINK_ANCHOR;")
    endif()

    # Standalone, the image crate is its own cargo root. With WORKSPACE_DIR it
    # is a MEMBER of one generated root shared with its sibling (the C and C++
    # images of one build), so one target dir serves one root (issue 0616).
    # Either way the ROOT carries the root workspace's profiles (issue 1953):
    # before D9 the API crate was built inside that workspace, and a generated
    # root states every profile it is built with or gets cargo's defaults.
    nros_root_cargo_profiles(_profiles REPO "${_repo}")
    set(_ws_table "\n[workspace]\n${_profiles}")
    set(_lock_root "${_IC_OUT_DIR}")
    if(_IC_WORKSPACE_DIR)
        set(_ws_table "")
        set(_lock_root "${_IC_WORKSPACE_DIR}")
        get_property(_members GLOBAL PROPERTY NROS_IMAGE_WORKSPACE_MEMBERS)
        file(RELATIVE_PATH _rel "${_IC_WORKSPACE_DIR}" "${_IC_OUT_DIR}")
        list(APPEND _members "${_rel}")
        list(REMOVE_DUPLICATES _members)
        list(SORT _members)
        set_property(GLOBAL PROPERTY NROS_IMAGE_WORKSPACE_MEMBERS "${_members}")
        set(_mlist "")
        foreach(_m IN LISTS _members)
            string(APPEND _mlist "\"${_m}\", ")
        endforeach()
        set(_root_toml
"# GENERATED by cmake/NanoRosImageCrate.cmake (RFC-0071 D9) — do not edit.
[workspace]
resolver = \"2\"
members = [${_mlist}]
${_profiles}")
        file(MAKE_DIRECTORY "${_IC_WORKSPACE_DIR}")
        set(_old_root "")
        if(EXISTS "${_IC_WORKSPACE_DIR}/Cargo.toml")
            file(READ "${_IC_WORKSPACE_DIR}/Cargo.toml" _old_root)
        endif()
        if(NOT _old_root STREQUAL _root_toml)
            file(WRITE "${_IC_WORKSPACE_DIR}/Cargo.toml" "${_root_toml}")
        endif()
    endif()
    file(MAKE_DIRECTORY "${_IC_OUT_DIR}/src")
    set(_manifest
"# GENERATED by cmake/NanoRosImageCrate.cmake (RFC-0071 D9) — do not edit.
#
# This image's Rust staticlib: `${_api_pkg}` with no backend feature, plus the
# backend crate its descriptor names. The lib is named `${_api_lib}` so the
# Corrosion targets keep the API crate's names.
[package]
name = \"${_api_pkg}-image\"
version = \"0.0.0\"
edition = \"2024\"
publish = false

[lib]
name = \"${_api_lib}\"
path = \"src/lib.rs\"
crate-type = [\"staticlib\"]

[dependencies]
api = { package = \"${_api_pkg}\", path = \"${_api_dir}\", default-features = false }
${_ib_DEP_LINE}

[features]
${_forward}${_ws_table}")
    set(_lib
"// GENERATED by cmake/NanoRosImageCrate.cmake (RFC-0071 D9) — do not edit.
//
// The staticlib root of this image. The API crate is a dependency here, so its
// own `#[used]` anchors are dropped before this root is emitted; these re-pull
// its C surface and the backend's registration (the generated strong
// `nros_app_register_backends()` calls the backend's C register symbol).
${_no_std}#![allow(unused)]
#[used]
${_surface}
${_ib_ANCHOR}")
    # Write only on change: an unchanged manifest must not re-trigger cargo.
    foreach(_pair "Cargo.toml|_manifest" "src/lib.rs|_lib")
        string(REPLACE "|" ";" _pv "${_pair}")
        list(GET _pv 0 _rel)
        list(GET _pv 1 _var)
        set(_path "${_IC_OUT_DIR}/${_rel}")
        set(_old "")
        if(EXISTS "${_path}")
            file(READ "${_path}" _old)
        endif()
        if(NOT _old STREQUAL "${${_var}}")
            file(WRITE "${_path}" "${${_var}}")
        endif()
    endforeach()

    # A generated root is a leaf: its lock is SEEDED from the registry pins
    # (RFC-0104 D3), never resolved fresh. The seed holds the REGISTRY versions
    # only; the path packages (the API crate, the backend, nano-ros's own
    # crates) have no entry until cargo adds them, which `--locked` forbids. So
    # the import runs without LOCKED, as `nros_ws_runtime` always has: cargo
    # keeps every seeded version and only adds the local entries.
    if(NOT EXISTS "${_lock_root}/Cargo.lock")
        _nros_rmw_cli(_tool)
        set(_seed_rc 1)
        if(_tool)
            # A configure-time emitter: a rebuilt `nros` must re-run it (issue 1018).
            nros_codegen_tool_reconfigure("${_tool}")
            execute_process(
                COMMAND "${_tool}" ws seed-lock "${_lock_root}" --nano-ros-path "${_repo}"
                RESULT_VARIABLE _seed_rc OUTPUT_QUIET ERROR_VARIABLE _seed_err)
        endif()
        if(NOT _seed_rc EQUAL 0)
            message(STATUS
                "nano-ros: could not seed ${_lock_root}/Cargo.lock from the pins "
                "(${_seed_err}); cargo resolves the image crate unseeded.")
        endif()
    endif()

    message(STATUS
        "nano-ros: ${_api_pkg} image crate bundles ${_bundles} "
        "(RFC-0071 D9) -> ${_IC_OUT_DIR}")
    set(${prefix}_MANIFEST "${_IC_OUT_DIR}/Cargo.toml" PARENT_SCOPE)
    set(${prefix}_CRATE "${_api_pkg}-image" PARENT_SCOPE)
    set(${prefix}_FEATURES "${_api_feats}" PARENT_SCOPE)
    set(${prefix}_LOCKED "" PARENT_SCOPE)
endfunction()

if(CMAKE_SCRIPT_MODE_FILE AND
   CMAKE_SCRIPT_MODE_FILE STREQUAL CMAKE_CURRENT_LIST_FILE)
    foreach(_v FLAVOR RMW EDITION OUT_DIR API_FEATURES)
        if(NOT DEFINED NROS_IMAGE_${_v})
            message(FATAL_ERROR
                "usage: cmake -DNROS_IMAGE_FLAVOR=c|cpp -DNROS_IMAGE_RMW=<rmw> "
                "-DNROS_IMAGE_EDITION=<edition> -DNROS_IMAGE_OUT_DIR=<dir> "
                "[-DNROS_IMAGE_WORKSPACE_DIR=<dir>] \"-DNROS_IMAGE_API_FEATURES=<f;f>\" "
                "-P cmake/NanoRosImageCrate.cmake (missing NROS_IMAGE_${_v})")
        endif()
    endforeach()
    # The edition's backend destination (issue 1893), which a CMake road gets
    # from nros_feature_set() as a `<rlib>/<feature>` row.
    set(_feats ${NROS_IMAGE_API_FEATURES})
    if(NOT NROS_IMAGE_RMW STREQUAL "" AND NOT NROS_IMAGE_RMW STREQUAL "none")
        nros_rmw_dispatch("${NROS_IMAGE_RMW}")
        if(NROS_RMW_RLIB_DEP AND NROS_RMW_EDITION_FEATURE)
            string(REPLACE "{edition}" "${NROS_IMAGE_EDITION}" _ed "${NROS_RMW_EDITION_FEATURE}")
            list(APPEND _feats "${NROS_RMW_RLIB_DEP}/${_ed}")
        endif()
    endif()
    set(_ws "")
    if(DEFINED NROS_IMAGE_WORKSPACE_DIR)
        set(_ws WORKSPACE_DIR "${NROS_IMAGE_WORKSPACE_DIR}")
    endif()
    nros_image_crate(_img ALWAYS FLAVOR "${NROS_IMAGE_FLAVOR}" RMW "${NROS_IMAGE_RMW}"
        OUT_DIR "${NROS_IMAGE_OUT_DIR}" ${_ws} API_FEATURES ${_feats})
    string(REPLACE ";" "," _out_feats "${_img_FEATURES}")
    execute_process(COMMAND "${CMAKE_COMMAND}" -E echo "${_img_MANIFEST} ${_out_feats}")
endif()

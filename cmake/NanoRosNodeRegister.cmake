# cmake/NanoRosNodeRegister.cmake — Phase 212.L.9 / 212.N.6
#
# C++ cmake fn surface for the three Phase 212.L pkg shapes:
#
#   * `nano_ros_node_register(NAME <name> CLASS <UserClass>
#       [LANGUAGE C|CPP|RUST] SOURCES <files...> DEPLOY <target1> [<target2> ...])`
#       — declares a Component pkg entity. Compiles SOURCES into a
#         STATIC `<pkg>_<name>_component` lib linked to the C or C++
#         nano-ros target. Rust packages import `Cargo.toml` through
#         Corrosion and expose the same component target name for entry
#         link glue. Enforces L.4: CLASS must start with `${PROJECT_NAME}::`.
#
#   * `nano_ros_entry(NAME <name> SOURCES <files...> [BOARD <board>]
#       DEPLOY <target1> [<target2> ...])`
#       — declares an Entry pkg entity. Renamed from
#         `nano_ros_application` per Phase 212.L.9 / 212.N.6. Defined
#         in `NanoRosEntry.cmake` (auto-included below); see that
#         module for the body + the BOARD-arg semantics.
#
#   * `nano_ros_application(...)` — DEPRECATED 212.N.6 backward-compat
#       shim. Emits a `MESSAGE(DEPRECATION …)` and forwards every
#       argument to `nano_ros_entry`. The shim will be retired once
#       the in-tree caller migration (212.N.7 wave) completes.
#
#   * `nano_ros_component_register(...)` — DEPRECATED 213.B.1 backward-
#       compat shim. The Phase 212.N.12 hard rename swept
#       `Component → Node` across the code surface but missed this
#       cmake fn name, leaving every embedded C/C++ example calling it
#       failing at configure time. Emits `MESSAGE(DEPRECATION …)` and
#       forwards every argument to `nano_ros_node_register`. Retired
#       after the 213.B.2 caller sweep.
#
#   * `nano_ros_deploy(TARGET <name> RMW <rmw> DOMAIN_ID <n>
#       [LOCATOR <uri>])`
#       — records per-target rmw / domain_id / locator config.
#
# Side effect: every fn appends to GLOBAL props and rewrites
# `${CMAKE_BINARY_DIR}/nros-metadata.json` so `nros codegen-system`
# can consume it at configure time. No sidecar TOML for C++ pkgs.
#
# The function is deliberately declarative/glue-only; entry generation
# lives in `NanoRosEntry.cmake`.

if(DEFINED _NROS_NODE_REGISTER_INCLUDED)
    return()
endif()
set(_NROS_NODE_REGISTER_INCLUDED TRUE)

# Capture this module's directory at include time. `CMAKE_CURRENT_LIST_DIR`
# is dynamic — inside a function body it resolves to the *caller's* list
# dir, not this file's — so the Phase 238 carrier `configure_file` must use
# this captured path to find `templates/nuttx_entry_main.cpp.in`.
set(_NROS_NODE_REGISTER_DIR "${CMAKE_CURRENT_LIST_DIR}")

define_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON
    BRIEF_DOCS "Accumulated component JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_node_register().")
define_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON
    BRIEF_DOCS "Accumulated application JSON fragments"
    FULL_DOCS  "Phase 212.L.9 / 212.N.6 — appended by nano_ros_entry().")
define_property(GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON
    BRIEF_DOCS "Accumulated deploy_targets JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_deploy().")
set_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON "")
set_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON "")
set_property(GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON "")

# Emit the JSON file. Idempotent — called after every fn so the file
# is always current. Keep small: writes the whole doc each time.
function(_nros_metadata_emit)
    get_property(_comps   GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    get_property(_apps    GLOBAL PROPERTY NROS_APPLICATIONS_JSON)
    get_property(_targets GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON)
    set(_doc "{\n")
    string(APPEND _doc "  \"components\": [${_comps}\n  ],\n")
    string(APPEND _doc "  \"applications\": [${_apps}\n  ],\n")
    string(APPEND _doc "  \"deploy_targets\": {${_targets}\n  }\n")
    string(APPEND _doc "}\n")
    file(WRITE "${CMAKE_BINARY_DIR}/nros-metadata.json" "${_doc}")
endfunction()

# Helper: render a string list as a JSON array body.
function(_nros_json_strlist out_var)
    set(_acc "")
    set(_first TRUE)
    foreach(_v IN LISTS ARGN)
        if(_first)
            set(_acc "\"${_v}\"")
            set(_first FALSE)
        else()
            set(_acc "${_acc}, \"${_v}\"")
        endif()
    endforeach()
    set(${out_var} "${_acc}" PARENT_SCOPE)
endfunction()

function(nano_ros_node_register)
    cmake_parse_arguments(_NRC "" "NAME;CLASS;LANGUAGE" "SOURCES;DEPLOY" ${ARGN})
    foreach(_req NAME CLASS SOURCES DEPLOY)
        if(NOT _NRC_${_req})
            message(FATAL_ERROR
                "nano_ros_node_register: ${_req} required")
        endif()
    endforeach()
    if(_NRC_LANGUAGE)
        string(TOUPPER "${_NRC_LANGUAGE}" _nrc_lang)
    else()
        # Back-compat: old C examples omitted LANGUAGE. If every source is a C
        # TU, record/link it as C; otherwise preserve the historical C++ default.
        set(_nrc_lang C)
        foreach(_src IN LISTS _NRC_SOURCES)
            get_filename_component(_ext "${_src}" EXT)
            string(TOLOWER "${_ext}" _ext_lc)
            if(NOT _ext_lc STREQUAL ".c")
                set(_nrc_lang CPP)
            endif()
        endforeach()
    endif()
    if(_nrc_lang STREQUAL "CXX")
        set(_nrc_lang CPP)
    endif()
    if(_nrc_lang STREQUAL "RUST" OR _nrc_lang STREQUAL "RS")
        set(_nrc_lang RUST)
    endif()
    if(NOT (_nrc_lang STREQUAL "C" OR _nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "RUST"))
        message(FATAL_ERROR
            "nano_ros_node_register: LANGUAGE '${_NRC_LANGUAGE}' rejected — "
            "expected C, CPP, or RUST")
    endif()
    string(TOLOWER "${_nrc_lang}" _nrc_lang_lc)
    # L.4 enforcement: CLASS must start with `${PROJECT_NAME}::`.
    string(FIND "${_NRC_CLASS}" "${PROJECT_NAME}::" _idx)
    if(NOT _idx EQUAL 0)
        message(FATAL_ERROR
            "nano_ros_node_register: CLASS '${_NRC_CLASS}' must "
            "start with '${PROJECT_NAME}::' (Phase 212.L.4 rule — the "
            "pkg directory name IS the pkg name).")
    endif()

    set(_lib "${PROJECT_NAME}_${_NRC_NAME}_component")
    if(NOT TARGET ${_lib})
        # Phase 212.M.5.a.1 — package symbol used by C/C++ macros and
        # mirrored by Rust `nros::node!()`.
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")

        if(_nrc_lang STREQUAL "RUST")
            if(NOT COMMAND corrosion_import_crate)
                message(FATAL_ERROR
                    "nano_ros_node_register(LANGUAGE RUST): Corrosion is required. "
                    "Build via nano_ros_workspace()/add_subdirectory(nano-ros) so "
                    "the in-tree Corrosion dependency is available.")
            endif()
            if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")
                message(FATAL_ERROR
                    "nano_ros_node_register(LANGUAGE RUST): expected Cargo.toml "
                    "in ${CMAKE_CURRENT_SOURCE_DIR}")
            endif()
            corrosion_import_crate(
                MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
                CRATES        "${PROJECT_NAME}"
                CRATE_TYPES   staticlib
            )
            string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _crate_target "${PROJECT_NAME}")
            set(_rust_static_target "${_crate_target}-static")
            if(NOT TARGET ${_rust_static_target})
                message(FATAL_ERROR
                    "nano_ros_node_register(LANGUAGE RUST): Corrosion did not "
                    "create target '${_rust_static_target}'. Ensure the package "
                    "name matches project(${PROJECT_NAME}) and [lib] includes "
                    "crate-type = [\"staticlib\", ...].")
            endif()
            add_library(${_lib} INTERFACE)
            target_link_libraries(${_lib} INTERFACE ${_rust_static_target})
        else()
            add_library(${_lib} STATIC ${_NRC_SOURCES})
            if(_nrc_lang STREQUAL "C")
                set_target_properties(${_lib} PROPERTIES LINKER_LANGUAGE C)
            endif()
            if(_nrc_lang STREQUAL "C" AND TARGET NanoRos::NanoRos)
                target_link_libraries(${_lib} PUBLIC NanoRos::NanoRos)
            elseif(TARGET NanoRos::NanoRosCpp)
                target_link_libraries(${_lib} PUBLIC NanoRos::NanoRosCpp)
            endif()
            target_include_directories(${_lib} PUBLIC
                "${CMAKE_CURRENT_SOURCE_DIR}/include"
                "${CMAKE_CURRENT_SOURCE_DIR}/src")
            target_compile_definitions(${_lib} PRIVATE
                NROS_PKG_NAME=${_pkg_sym}
                "NROS_NODE_CLASS_NAME=\"${_NRC_CLASS}\"")
        endif()

        # Phase 220.G.2 — auto-link every `<pkg>__nano_ros_{c,cpp}`
        # interface lib that `nros_generate_interfaces` registered in
        # this directory's scope. Without this, an example whose src
        # `#include "std_msgs.h"` (or `.hpp`) fails with
        # `No such file or directory` because the include dirs live on
        # the interface lib's INTERFACE_INCLUDE_DIRECTORIES. Pre-220.G
        # every example had to append a per-pkg manual
        # `target_link_libraries(<component> PUBLIC <pkg>__nano_ros_X)`
        # (the 220.G.1 boilerplate, now revertible).
        # DIRECTORY scope — see the property write in
        # NanoRosGenerateInterfaces.cmake.
        if(NOT _nrc_lang STREQUAL "RUST")
            get_directory_property(_nros_iface_libs NROS_GENERATED_INTERFACE_LIBS)
            if(_nros_iface_libs)
                list(REMOVE_DUPLICATES _nros_iface_libs)
                target_link_libraries(${_lib} PUBLIC ${_nros_iface_libs})
            endif()
        endif()
    endif()

    # Phase 238 — NuttX bootable-ELF carrier. The Component lib above is
    # build-coverage only; the rtos_e2e harness + `build_nuttx_cpp_*`
    # resolvers need a bootable kernel ELF at `build-zenoh/<PROJECT_NAME>`.
    # When this Node pkg deploys to nuttx AND the NuttX platform/board
    # overlay is active (`nros_platform_link_app` defined), synthesise a
    # single-node entry TU + a carrier `add_executable(<PROJECT_NAME> …)`
    # and delegate to `nros_platform_link_app` (→ `nros_board_link_app` →
    # `nros_nuttx_build_example`), which drives the cargo `nros-nuttx-ffi`
    # kernel link and copies the ELF to `build-zenoh/<PROJECT_NAME>`.
    #
    # Scope: pub/sub (talker/listener), C AND C++ (238.C). The generated
    # entry is ALWAYS C++ (it drives the header-only C++ EntryNodeRuntime);
    # a C example's declarative node (`Talker.c`) is added as an extra source
    # and compiled as C by the mixed-language cargo build
    # (nros-board-common::nuttx_ffi_build), so its C-linkage
    # `__nros_component_<pkg>_register` symbol matches the entry's
    # `extern "C"` decl. Services / actions register but do not execute
    # (interpreter limit; deferred — see phase-238).
    if((_nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "C")
       AND "nuttx" IN_LIST _NRC_DEPLOY
       AND NANO_ROS_PLATFORM STREQUAL "nuttx"
       AND COMMAND nros_platform_link_app
       AND NOT TARGET ${PROJECT_NAME})
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")
        set(NROS_ENTRY_PKG_SYM "${_pkg_sym}")
        # Baked connect locator. QEMU slirp routes the guest to the host
        # zenoh router at `10.0.2.2:<port>`. Override per-build with
        # `-DNROS_NUTTX_LOCATOR=tcp/10.0.2.2:<port>` (the rtos_e2e harness
        # passes the per-cell `zenohd_port_for` port); the default 7447
        # serves manual `zenohd` runs. Mirrors the Rust `*_entry`
        # `[…entry] locator = …` bake.
        if(NOT DEFINED NROS_NUTTX_LOCATOR)
            set(NROS_NUTTX_LOCATOR "tcp/10.0.2.2:7447")
        endif()
        set(NROS_ENTRY_LOCATOR "${NROS_NUTTX_LOCATOR}")
        set(_entry_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-entry")
        set(_entry_src "${_entry_dir}/main.cpp")
        configure_file(
            "${_NROS_NODE_REGISTER_DIR}/templates/nuttx_entry_main.cpp.in"
            "${_entry_src}" @ONLY)

        # Carrier executable named after the pkg so the ELF lands at
        # `build-zenoh/${PROJECT_NAME}`. SOURCES = entry (main.cpp, picked
        # up as MAIN_SOURCE by nros_board_link_app's `/main\.cpp$` match) +
        # the Component class source(s) (compiled as APP_EXTRA_SOURCES).
        add_executable(${PROJECT_NAME} "${_entry_src}" ${_NRC_SOURCES})
        target_include_directories(${PROJECT_NAME} PRIVATE
            "${CMAKE_CURRENT_SOURCE_DIR}/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/src")
        # NROS_PKG_NAME reaches the class TU through nros_board_link_app's
        # COMPILE_DEFINITIONS → APP_COMPILE_DEFS forwarding (Phase 238).
        target_compile_definitions(${PROJECT_NAME} PRIVATE
            NROS_PKG_NAME=${_pkg_sym})
        if(TARGET NanoRos::NanoRosCpp)
            target_link_libraries(${PROJECT_NAME} PRIVATE NanoRos::NanoRosCpp)
        elseif(TARGET NanoRos::NanoRos)
            target_link_libraries(${PROJECT_NAME} PRIVATE NanoRos::NanoRos)
        endif()
        get_directory_property(_nros_iface_libs NROS_GENERATED_INTERFACE_LIBS)
        if(_nros_iface_libs)
            list(REMOVE_DUPLICATES _nros_iface_libs)
            target_link_libraries(${PROJECT_NAME} PRIVATE ${_nros_iface_libs})
        endif()
        nros_platform_link_app(${PROJECT_NAME})
    endif()

    _nros_json_strlist(_sources_json ${_NRC_SOURCES})
    _nros_json_strlist(_deploy_json  ${_NRC_DEPLOY})
    get_property(_acc GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    {\"name\": \"${_NRC_NAME}\", \"class\": \"${_NRC_CLASS}\", \
\"sources\": [${_sources_json}], \"deploy\": [${_deploy_json}], \
\"pkg_dir\": \"${CMAKE_CURRENT_SOURCE_DIR}\", \"lang\": \"${_nrc_lang_lc}\"}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_COMPONENTS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()

# Phase 212.N.6 — backward-compat shim. `nano_ros_application` was
# renamed to `nano_ros_entry` per L.9 + N.6; this shim forwards every
# argument to the new fn and emits a DEPRECATION warning so callers
# can be migrated incrementally (tracked under 212.N.7). Slated for
# removal once the in-tree caller sweep lands.
function(nano_ros_application)
    message(DEPRECATION
        "nano_ros_application is renamed to nano_ros_entry — use "
        "nano_ros_entry(...) instead. The shim will be retired in a "
        "future phase (212.N.7 caller migration).")
    nano_ros_entry(${ARGV})
endfunction()

# Phase 213.B.1 — backward-compat shim. `nano_ros_component_register`
# was renamed to `nano_ros_node_register` per the 212.N.12 hard rename,
# which swept `Component → Node` across the code surface but missed
# this cmake fn name — leaving every embedded C/C++ example calling it
# failing at configure time. This shim forwards every argument to the
# new fn and emits a DEPRECATION warning. Retired after the 213.B.2
# caller sweep lands.
function(nano_ros_component_register)
    message(DEPRECATION
        "nano_ros_component_register is renamed to "
        "nano_ros_node_register — use nano_ros_node_register(...) "
        "instead. The shim will be retired in a future release.")
    nano_ros_node_register(${ARGV})
endfunction()

function(nano_ros_deploy)
    cmake_parse_arguments(_NRD "" "TARGET;RMW;DOMAIN_ID;LOCATOR" "" ${ARGN})
    foreach(_req TARGET RMW DOMAIN_ID)
        if(NOT DEFINED _NRD_${_req})
            message(FATAL_ERROR
                "nano_ros_deploy: ${_req} required")
        endif()
    endforeach()
    if(DEFINED _NRD_LOCATOR AND NOT _NRD_LOCATOR STREQUAL "")
        set(_loc_json "\"${_NRD_LOCATOR}\"")
    else()
        set(_loc_json "null")
    endif()

    get_property(_acc GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    \"${_NRD_TARGET}\": {\"rmw\": \"${_NRD_RMW}\", \
\"domain_id\": ${_NRD_DOMAIN_ID}, \"locator\": ${_loc_json}}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_DEPLOY_TARGETS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()

# Phase 212.N.6 — pull in `nano_ros_entry`. The Entry module
# back-includes this file (guarded) for the shared helpers
# (`_nros_metadata_emit`, `_nros_json_strlist`) + GLOBAL property
# definitions; doing the include LAST ensures those helpers are
# already defined by the time NanoRosEntry's body runs, and that the
# deprecation shim above can resolve `nano_ros_entry` at call time.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosEntry.cmake")

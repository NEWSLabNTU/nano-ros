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
#   (The `nano_ros_application` / `nano_ros_component_register` deprecation
#   shims were retired in 287-W8; `nano_ros_deploy` post-287 — the per-package
#   deploy/rmw tuple lives in package.xml `<export><nano_ros …/>`.)
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
# CACHE INTERNAL (like NanoRosEntry's `_NROS_ENTRY_DIR`): the workspace path
# includes this module inside a FUNCTION scope (`_nros_import_once`), so a
# normal variable dies with that frame and a member pkg's embedded-carrier
# `configure_file` resolved `/templates/…` (287-W6: every freertos workspace
# member failed "File /templates/freertos_entry_main_c_typed.cpp.in does not
# exist"; posix never touches the templates, which hid it).
set(_NROS_NODE_REGISTER_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE INTERNAL
    "nano_ros_node_register module dir")

# Issue 0088 — order any target whose TUs `#include <nros/nros_config_generated.h>`
# (or the C++ variant) AFTER the per-build header generators. The headers are
# produced by the nros-c / nros-cpp corrosion builds and mirrored into the in-tree
# `include/nros/` dir by the `nros_{c,cpp}_config_header` custom targets (OUTPUT
# edges, not POST_BUILD side effects). Depending only on `cargo-build_nros_{c,cpp}`
# is NOT enough: that orders against the cargo primary output, so a consumer TU can
# still compile before the mirror copy runs and pick up the in-tree stub header.
#
# The wiring is DEFERRED to end-of-configure: under `add_subdirectory(<repo-root>)`
# (Zephyr / examples) the consumer's `nano_ros_node_register()` often runs BEFORE
# the nros-c / nros-cpp subdirs are added, so the generator targets don't exist yet
# and an immediate `add_dependencies` would silently no-op (the `if(TARGET)` guard
# skips) — leaving the race in place (pass/fail purely by build order). A deferred
# call (`cmake_language(DEFER)` at the top-level dir) runs after every subdir is
# processed, when all targets exist, so the edge is always applied.
function(_nros_node_register_apply_config_header_deps _tgt)
    if(NOT TARGET ${_tgt})
        return()
    endif()
    # Two header-generation flavours, depending on the consuming build:
    #  * Native / examples (add_subdirectory(<repo-root>) → Corrosion): the
    #    `cargo-build_nros_{c,cpp}` cargo targets + the `nros_{c,cpp}_config_header`
    #    OUTPUT-mirror targets (packages/core/nros-{c,cpp}/CMakeLists.txt).
    #  * Zephyr (`zephyr/CMakeLists.txt` → `nros_cargo_build`): the
    #    `nros_{c,cpp}_cargo_build` targets, which emit the per-build header into
    #    `${CMAKE_BINARY_DIR}/nros-rust/nros-{c,cpp}-generated`. `app` already
    #    depends on these, but a `nano_ros_node_register` component lib is a
    #    SEPARATE target and otherwise races them → in-tree stub (issue 0088).
    # Guarded by `if(TARGET)`, so each flavour contributes only where it exists.
    foreach(_dep cargo-build_nros_cpp cargo-build_nros_c
                 nros_cpp_config_header nros_c_config_header
                 nros_cpp_cargo_build nros_c_cargo_build)
        if(TARGET ${_dep})
            add_dependencies(${_tgt} ${_dep})
        endif()
    endforeach()
endfunction()

function(_nros_node_register_config_header_deps _tgt)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nros_node_register_apply_config_header_deps "${_tgt}")
endfunction()

define_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON
    BRIEF_DOCS "Accumulated component JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_node_register().")
define_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON
    BRIEF_DOCS "Accumulated application JSON fragments"
    FULL_DOCS  "Phase 212.L.9 / 212.N.6 — appended by nano_ros_entry().")
set_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON "")
set_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON "")

# Emit the JSON file. Idempotent — called after every fn so the file
# is always current. Keep small: writes the whole doc each time.
function(_nros_metadata_emit)
    get_property(_comps   GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    get_property(_apps    GLOBAL PROPERTY NROS_APPLICATIONS_JSON)
    set(_doc "{\n")
    string(APPEND _doc "  \"components\": [${_comps}\n  ],\n")
    string(APPEND _doc "  \"applications\": [${_apps}\n  ]\n")
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
    cmake_parse_arguments(_NRC "TYPED" "NAME;CLASS;LANGUAGE;HEADER;SHAPE" "SOURCES;DEPLOY;CALLBACK_GROUPS" ${ARGN})
    # Phase 248 C6b (#60 T5) — DEPLOY is OPTIONAL on a Node pkg. A reusable Node
    # pkg must NOT name a deploy target; the Entry pkg (`nano_ros_entry(... DEPLOY
    # …)`) + the bringup `system.toml` select RMW/platform/deploy. Embedded Node
    # pkgs that drive a single-node carrier (NuttX/ThreadX/Zephyr branches below)
    # still pass `DEPLOY <rtos>` — those branches gate on `<rtos> IN_LIST
    # _NRC_DEPLOY`, so absence is a no-op (the metadata `deploy` array is empty
    # and the Entry/system.toml is the selection point).
    foreach(_req NAME CLASS SOURCES)
        if(NOT _NRC_${_req})
            message(FATAL_ERROR
                "nano_ros_node_register: ${_req} required")
        endif()
    endforeach()
    # Phase 242.4 (RFC-0044) — component SHAPE: `rclcpp` (IS-A-node, ctor-wired,
    # construct-with-handle — the typed entry placement-news it with the executor
    # handle *after* init and checks `ok()`) or `configure` (RFC-0043, the
    # default/back-compat: default-construct + `configure(node)`). Recorded in the
    # metadata JSON (the CLI `emit_typed` reads it onto `PlanNode.shape`) AND
    # surfaced to the carrier template as `NROS_ENTRY_SHAPE_RCLCPP` (0|1).
    if(_NRC_SHAPE)
        string(TOLOWER "${_NRC_SHAPE}" _nrc_shape)
    else()
        set(_nrc_shape "configure")
    endif()
    if(NOT (_nrc_shape STREQUAL "rclcpp" OR _nrc_shape STREQUAL "configure"))
        message(FATAL_ERROR
            "nano_ros_node_register: SHAPE '${_NRC_SHAPE}' rejected — "
            "expected rclcpp or configure")
    endif()
    if(_nrc_shape STREQUAL "rclcpp")
        set(_nrc_shape_rclcpp 1)
    else()
        set(_nrc_shape_rclcpp 0)
    endif()
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

    # Phase 240.2b (RFC-0043) — the typed Entry emitter `#include`s the
    # component's class header to construct it. Accept an explicit HEADER or
    # derive it from CLASS by convention: `pkg::Sub::Class` → `pkg/Sub/Class.hpp`
    # (namespace `::` → `/`, `.hpp` suffix), which resolves against the component
    # lib's `include/` (added to its PUBLIC include dirs below). Recorded in the
    # metadata JSON so the codegen can populate `PlanNode.class_header`.
    if(_NRC_HEADER)
        set(_nrc_header "${_NRC_HEADER}")
    else()
        string(REPLACE "::" "/" _nrc_header "${_NRC_CLASS}")
        set(_nrc_header "${_nrc_header}.hpp")
    endif()

    # Issue 0088 (Zephyr path) — on Zephyr the per-build nros config headers are
    # BYPRODUCTS of an always-run `nros_{c,cpp}_cargo_build` target
    # (zephyr/cmake/nros_cargo_build.cmake), written into
    # `${CMAKE_BINARY_DIR}/nros-rust/nros-{c,cpp}-generated`. The Zephyr component
    # lib is a plain `add_library` that inherits the generated + stub include dirs
    # (via `zephyr_include_directories`) but has no hard edge to the byproduct, so
    # its TUs can compile before the cargo build writes the real header and pick up
    # the in-tree stub (`#error "must be supplied per-build"`). `add_dependencies`
    # on the cargo target proved insufficient across the configure-order / typed-C
    # matrix; a file-level `OBJECT_DEPENDS` is a HARD Ninja edge — the TU won't
    # compile until the byproduct exists — independent of target name / order.
    # The C-variant header is a byproduct of EVERY nros Zephyr build (declared on
    # nros-c, or on nros-cpp when nros-c isn't built separately), so listing it is
    # always safe; depending on it also transitively orders after the same cargo
    # target that writes the C++ variant, covering C / typed-C / C++ consumers.
    if(NANO_ROS_PLATFORM STREQUAL "zephyr" AND _NRC_SOURCES)
        set_source_files_properties(${_NRC_SOURCES} PROPERTIES OBJECT_DEPENDS
            "${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated/nros/nros_config_generated.h;${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated/nros/nros_generated.h")
    endif()

    set(_lib "${PROJECT_NAME}_${_NRC_NAME}_component")
    if(NOT TARGET ${_lib})
        # Phase 212.M.5.a.1 — package symbol used by C/C++ macros and
        # mirrored by Rust `nros::node!()`.
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")

        if(_nrc_lang STREQUAL "RUST")
            # Phase 241 W11 (Option D) — a Rust Node pkg is NO LONGER imported as its own
            # Corrosion staticlib. A per-node `lib<pkg>.a` bundles its full `nros` closure
            # (incl. `nros-rmw-cffi`'s `#[no_mangle]` C ABI + REGISTRY); linked beside the
            # umbrella it collided (`multiple definition`) once single-runtime dropped
            # `--allow-multiple-definition`, and split the stateful REGISTRY (issue the W1
            # un-gate closed). Instead the workspace's per-configure runtime umbrella
            # (`nros_ws_runtime`, synthesised by `nano_ros_workspace`) bundles this node as
            # a cargo **rlib** path-dep — one Rust staticlib for the whole binary.
            #
            # `${_lib}` stays as an EMPTY INTERFACE target so the CLI-emitted entry
            # auto-link sidecar (`target_link_libraries(<entry> PRIVATE
            # <pkg>_<exec>_component)`, Phase 219.J) resolves to a harmless no-op — the
            # node's `__nros_component_<pkg>_register` symbol arrives via the runtime
            # umbrella, which the entry already links through `NanoRos::NanoRosCpp`.
            if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")
                message(FATAL_ERROR
                    "nano_ros_node_register(LANGUAGE RUST): expected Cargo.toml "
                    "in ${CMAKE_CURRENT_SOURCE_DIR}")
            endif()
            add_library(${_lib} INTERFACE)
        else()
            add_library(${_lib} STATIC ${_NRC_SOURCES})
            if(_nrc_lang STREQUAL "C")
                set_target_properties(${_lib} PROPERTIES LINKER_LANGUAGE C)
            endif()
            # Phase 215.J / 242 — on Zephyr the component lib is a plain
            # add_library(STATIC), so unlike the `find_package(Zephyr)`-owned
            # `app` target it does NOT inherit Zephyr's compile context (the
            # C++ standard from CONFIG_STD_CPP17, the zephyr + autogen include
            # dirs, the CONFIG_* defines). Without it, C++ sources that compiled
            # in a monolithic Zephyr app (e.g. ASI's vendored autoware libs)
            # fail (default `-std` + missing zephyr headers). `zephyr_interface`
            # is the INTERFACE target carrying exactly that build context; link
            # it so the component sources compile identically to `app`.
            if(TARGET zephyr_interface)
                target_link_libraries(${_lib} PRIVATE zephyr_interface)
            endif()
            # Phase 242 — the per-build `<nros/nros_cpp_config_generated.h>` /
            # `<nros/nros_config_generated.h>` (storage sizes, etc.) are emitted
            # as byproducts of the nros-cpp / nros-c cargo builds into
            # `${CMAKE_BINARY_DIR}/nros-rust/nros-{cpp,c}-generated` (prepended
            # to the include path by zephyr/CMakeLists.txt). `app` already
            # depends on those targets, but this component lib is a SEPARATE
            # add_library; without the same dependency its TUs can compile
            # before the headers exist (clean-build race) and pick up the
            # in-tree stub header, which #errors. Order it after the generators.
            # (Target names are `cargo-build_nros_{cpp,c}` — the corrosion cargo
            # targets; the pre-257 names `nros_{cpp,c}_cargo_build` never matched.
            # Issue 0088: also depend on the `nros_{c,cpp}_config_header` mirror
            # targets — the cargo dep alone races the POST_BUILD-era copy.)
            _nros_node_register_config_header_deps(${_lib})
            # Phase 257 (W0-A) — a TYPED C component (`NROS_C_COMPONENT`) calls the
            # `nros_cpp_*` seam (publisher/subscription/timer), which lives in the
            # C++ umbrella (nros-cpp), so it links NanoRosCpp like a C++ component —
            # NOT the C-only NanoRos. (A legacy declarative C node keeps NanoRos.)
            # The umbrella bundles nros-c's C ABI, so `nros_*` C calls still resolve,
            # and only ONE Rust staticlib is linked (no double `std`/REGISTRY).
            if(_nrc_lang STREQUAL "C" AND NOT _NRC_TYPED AND TARGET NanoRos::NanoRos)
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
            # phase-263 C2b — record the pkg sym + language on the component lib so a
            # consumer that must RECOMPILE this node's sources with the right
            # `-DNROS_PKG_NAME` (the NuttX kernel link: cc-rs builds each component source
            # for the ARM target separately, since the host-built `.a` is the wrong arch)
            # can recover it without re-deriving from the target name. SOURCES + SOURCE_DIR
            # are standard properties the consumer also reads.
            set_target_properties(${_lib} PROPERTIES
                NROS_COMPONENT_PKG_SYM "${_pkg_sym}"
                NROS_COMPONENT_LANG "${_nrc_lang}")
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
            # phase-263 C2c — on Zephyr the generated interface FFI is whole-archived into
            # `app` directly (the Zephyr generator), NOT exposed as a per-pkg
            # `<pkg>__nano_ros_cpp` linkable lib, so `NROS_GENERATED_INTERFACE_LIBS` may carry
            # a NON-target name → the component link tries `-l<name>` and fails ("cannot find
            # -lstd_msgs__nano_ros_cpp"). Skip the interface-lib link on Zephyr; the component
            # gets the generated msg headers via the `app` include mirror below.
            if(NOT NANO_ROS_PLATFORM STREQUAL "zephyr")
                get_directory_property(_nros_iface_libs NROS_GENERATED_INTERFACE_LIBS)
                if(_nros_iface_libs)
                    list(REMOVE_DUPLICATES _nros_iface_libs)
                    target_link_libraries(${_lib} PUBLIC ${_nros_iface_libs})
                endif()
            endif()
            # Phase 244.C2 — on Zephyr the generated message include dirs
            # (std_msgs.hpp, example_interfaces, …) are added by the Zephyr
            # `nros_generate_interfaces` directly to `app` PRIVATE
            # (zephyr/cmake/nros_generate_interfaces.cmake:290), NOT via the
            # NROS_GENERATED_INTERFACE_LIBS interface-lib path that native/nuttx
            # use. This component lib is a SEPARATE add_library (not `app`), so it
            # never sees those headers and a TYPED component that #includes a
            # generated msg header fails (`std_msgs.hpp: No such file`). Mirror
            # `app`'s full include set onto it — it compiles the same TUs `app`
            # would. Genexpr → captured at generate time, so it picks up includes
            # `find_package(<msg pkg>)` adds to `app` after this point too.
            if(NANO_ROS_PLATFORM STREQUAL "zephyr" AND TARGET app)
                target_include_directories(${_lib} PRIVATE
                    $<TARGET_PROPERTY:app,INCLUDE_DIRECTORIES>)
                # phase-263 C2c — HARD file edge for the per-build sizes headers. A C++
                # component pulls the full nros-cpp surface (action/client/service →
                # `<nros/nros_{,cpp_}config_generated.h>`); on a PRISTINE multi-node build a
                # SEPARATE component lib can compile before the Zephyr cargo build emits those
                # headers, falling through the include path to the in-tree stub (`#error`).
                # `_nros_node_register_config_header_deps` (above) orders the target but ninja
                # can still start the object compile early (issue 0090); a file-level
                # OBJECT_DEPENDS forces each TU to wait for the generated headers. (A C node /
                # the single-node carrier compiles into `app`, which already depends on the
                # cargo build, so neither hits this.)
                set_source_files_properties(${_NRC_SOURCES} PROPERTIES OBJECT_DEPENDS
                    "${CMAKE_BINARY_DIR}/nros-rust/nros-cpp-generated/nros/nros_cpp_config_generated.h;${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated/nros/nros_config_generated.h")
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
        # Phase 240.3 (RFC-0043) — TYPED routes the carrier to the real
        # executor via the component object (`NuttxBoard::run_components`
        # constructs `CLASS` + calls `configure(node)`), instead of the legacy
        # register-symbol → `EntryNodeRuntime` interpreter. Substitution vars
        # `NROS_ENTRY_CLASS` / `NROS_ENTRY_CLASS_HEADER` / `NROS_ENTRY_NODE_NAME`
        # feed the typed template. C++ only (the C path is 240.4).
        if(_NRC_TYPED)
            set(NROS_ENTRY_NODE_NAME "${_NRC_NAME}")
            set(NROS_ENTRY_SHAPE_RCLCPP "${_nrc_shape_rclcpp}")
            if(_nrc_lang STREQUAL "CPP")
                set(NROS_ENTRY_CLASS "${_NRC_CLASS}")
                set(NROS_ENTRY_CLASS_HEADER "${_nrc_header}")
                configure_file(
                    "${_NROS_NODE_REGISTER_DIR}/templates/nuttx_entry_main_typed.cpp.in"
                    "${_entry_src}" @ONLY)
            elseif(_nrc_lang STREQUAL "C")
                # Phase 240.4 — C typed component. The entry TU is C++ but
                # constructs the C component via its `__nros_c_component_<pkg>_*`
                # factory/configure seam (NROS_C_COMPONENT). `NROS_ENTRY_PKG_SYM`
                # is already set above to the sanitized pkg.
                configure_file(
                    "${_NROS_NODE_REGISTER_DIR}/templates/nuttx_entry_main_c_typed.cpp.in"
                    "${_entry_src}" @ONLY)
            else()
                message(FATAL_ERROR
                    "nano_ros_node_register(TYPED): NuttX carrier supports "
                    "LANGUAGE C or CPP (got '${_nrc_lang}').")
            endif()
        else()
            # Phase 257 (Stage-3) — the non-TYPED NuttX carrier drove the retired
            # `EntryNodeRuntime` interpreter (`NuttxBoard::run`); it is gone. Every
            # NuttX Node pkg is now TYPED (`NROS_C_COMPONENT` / `configure`).
            message(FATAL_ERROR
                "nano_ros_node_register: the non-typed NuttX carrier is retired "
                "(phase-257). Pass `TYPED` (LANGUAGE C or CPP) for the real-executor "
                "carrier.")
        endif()

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
        # Issue 0088 — the carrier executable compiles ${_NRC_SOURCES} (C/C++ TUs
        # that include <nros/nros_config_generated.h>); order them after the header
        # mirror targets so they never pick up the in-tree stub.
        _nros_node_register_config_header_deps(${PROJECT_NAME})
        nros_platform_link_app(${PROJECT_NAME})
    endif()

    # Phase 246 (RFC-0043) — ThreadX typed-entry carrier. Mirrors the NuttX
    # branch above (bare-metal riscv64 + threadx-linux host sim both set
    # `NANO_ROS_PLATFORM threadx`): synthesise a single-node C++ entry TU that
    # routes the component to the real executor via `ThreadxBoard::run_components`
    # (construct `CLASS` + `configure(node)`), then delegate to
    # `nros_platform_link_app` for the kernel/netstack/startup link. The board's
    # `startup.c` dispatches to the entry's `app_main` inside the app thread, so
    # the typed entry's `NROS_APP_MAIN_REGISTER_VOID()` symbol is the boot target.
    #
    # TYPED-only: the legacy declarative-register + `NanoRosThreadxSystemCodegen`
    # NULL-context stub is retired on ThreadX (phase-246). Both C and C++.
    if((_nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "C")
       AND NANO_ROS_PLATFORM STREQUAL "threadx"
       AND COMMAND nros_platform_link_app
       AND _NRC_DEPLOY
       AND NOT TARGET ${PROJECT_NAME})
        if(NOT _NRC_TYPED)
            message(FATAL_ERROR
                "nano_ros_node_register: the ThreadX carrier requires TYPED — "
                "the RFC-0043 real-callback component path. The legacy "
                "declarative-register / NULL-context baker entry is retired on "
                "ThreadX (phase-246).")
        endif()
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")
        set(NROS_ENTRY_PKG_SYM "${_pkg_sym}")
        # Baked connect locator. QEMU slirp routes the guest to the host zenoh
        # router at `10.0.2.2:<port>`. Override with `-DNROS_THREADX_LOCATOR=…`;
        # the default 7553 matches the qemu-riscv64-threadx fixture port.
        # CycloneDDS ignores the locator (no router); domain id is compile-time.
        if(NOT DEFINED NROS_THREADX_LOCATOR)
            set(NROS_THREADX_LOCATOR "tcp/10.0.2.2:7553")
        endif()
        set(NROS_ENTRY_LOCATOR "${NROS_THREADX_LOCATOR}")
        set(NROS_ENTRY_NODE_NAME "${_NRC_NAME}")
        set(NROS_ENTRY_SHAPE_RCLCPP "${_nrc_shape_rclcpp}")
        set(_entry_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-entry")
        set(_entry_src "${_entry_dir}/main.cpp")
        if(_nrc_lang STREQUAL "CPP")
            set(NROS_ENTRY_CLASS "${_NRC_CLASS}")
            set(NROS_ENTRY_CLASS_HEADER "${_nrc_header}")
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/threadx_entry_main_typed.cpp.in"
                "${_entry_src}" @ONLY)
        else() # C
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/threadx_entry_main_c_typed.cpp.in"
                "${_entry_src}" @ONLY)
        endif()

        add_executable(${PROJECT_NAME} "${_entry_src}" ${_NRC_SOURCES})
        target_include_directories(${PROJECT_NAME} PRIVATE
            "${CMAKE_CURRENT_SOURCE_DIR}/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/src")
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
        # Issue 0088 — the carrier executable compiles ${_NRC_SOURCES} (C/C++ TUs
        # that include <nros/nros_config_generated.h>); order them after the header
        # mirror targets so they never pick up the in-tree stub.
        _nros_node_register_config_header_deps(${PROJECT_NAME})
        # Issue 0090 — `add_dependencies` (above) orders the TARGET but does not stop
        # a TU compiling before the per-build header is mirrored on the threadx link
        # path, so it reads the in-tree stub (`*_OPAQUE_U64S undeclared`) — exactly
        # the 0088 zephyr failure on yet another provisioning path. Add a HARD
        # file-level OBJECT_DEPENDS on the Corrosion mirror header (abs path exported
        # as a global by nros-{c,cpp}/CMakeLists), so the TU won't compile until it
        # exists. C carrier reads the C header; C++ reads both — list whichever the
        # build provides (harmless extra edge).
        if(_NRC_SOURCES)
            get_property(_nros_c_cfg_hdr GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)
            get_property(_nros_cpp_cfg_hdr GLOBAL PROPERTY NROS_CPP_CONFIG_HEADER_FILE)
            set(_nros_cfg_hdrs "")
            if(_nros_c_cfg_hdr)
                list(APPEND _nros_cfg_hdrs "${_nros_c_cfg_hdr}")
            endif()
            if(_nros_cpp_cfg_hdr)
                list(APPEND _nros_cfg_hdrs "${_nros_cpp_cfg_hdr}")
            endif()
            if(_nros_cfg_hdrs)
                set_source_files_properties(${_NRC_SOURCES} PROPERTIES
                    OBJECT_DEPENDS "${_nros_cfg_hdrs}")
            endif()
        endif()
        nros_platform_link_app(${PROJECT_NAME})
    endif()

    # Phase 240.6 (RFC-0043) — FreeRTOS typed-entry carrier. Mirrors the NuttX /
    # ThreadX branches above (NANO_ROS_PLATFORM freertos, QEMU MPS2-AN385 + lwIP):
    # synthesise a single-node C++ entry TU that routes the component to the real
    # executor via `FreertosBoard::run_components` (construct `CLASS` +
    # `configure(node)`), then delegate to `nros_platform_link_app` for the
    # kernel/lwIP/netif/startup link. The board's `startup.c` `_start` spawns the
    # app task + starts the scheduler; that task's `app_task_entry` brings up the
    # network + poll/zenoh tasks, then dispatches to the entry's `app_main`, so the
    # typed entry's `NROS_APP_MAIN_REGISTER_VOID()` symbol is the boot target —
    # same shape as the NuttX carrier (network is up by the time `app_main` runs).
    #
    # Unlike the Rust FreeRTOS path (which links the board crate's build.rs-emitted
    # NROS_APP_CONFIG), the cmake C/C++ carrier does not pull the Rust board crate,
    # so it generates the NROS_APP_CONFIG TU that startup.c reads (network +
    # scheduling) from `templates/freertos_app_config.c.in`.
    #
    # TYPED-only: the legacy declarative-register / NULL-context baker entry is
    # retired on FreeRTOS (phase-240.6). Both C and C++.
    if((_nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "C")
       AND NANO_ROS_PLATFORM STREQUAL "freertos"
       AND COMMAND nros_platform_link_app
       AND _NRC_DEPLOY
       AND NOT TARGET ${PROJECT_NAME})
        if(NOT _NRC_TYPED)
            message(FATAL_ERROR
                "nano_ros_node_register: the FreeRTOS carrier requires TYPED — "
                "the RFC-0043 real-callback component path. The legacy "
                "declarative-register / NULL-context baker entry is retired on "
                "FreeRTOS (phase-240.6).")
        endif()
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")
        set(NROS_ENTRY_PKG_SYM "${_pkg_sym}")
        # Baked connect locator. QEMU slirp routes the guest to the host zenoh
        # router at `10.0.2.2:<port>`. Override with `-DNROS_FREERTOS_LOCATOR=…`;
        # the default 7447 matches the qemu-arm-freertos example deploy + the
        # rtos_e2e harness's manual `zenohd` default.
        if(NOT DEFINED NROS_FREERTOS_LOCATOR)
            set(NROS_FREERTOS_LOCATOR "tcp/10.0.2.2:7447")
        endif()
        set(NROS_ENTRY_LOCATOR "${NROS_FREERTOS_LOCATOR}")
        set(NROS_ENTRY_NODE_NAME "${_NRC_NAME}")
        set(NROS_ENTRY_SHAPE_RCLCPP "${_nrc_shape_rclcpp}")
        set(_entry_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-entry")
        set(_entry_src "${_entry_dir}/main.cpp")
        if(_nrc_lang STREQUAL "CPP")
            set(NROS_ENTRY_CLASS "${_NRC_CLASS}")
            set(NROS_ENTRY_CLASS_HEADER "${_nrc_header}")
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/freertos_entry_main_typed.cpp.in"
                "${_entry_src}" @ONLY)
        else() # C
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/freertos_entry_main_c_typed.cpp.in"
                "${_entry_src}" @ONLY)
        endif()

        # NROS_APP_CONFIG definition TU (network + scheduling) for startup.c.
        # `.zenoh.locator` is cosmetic on the typed path; bake the entry locator
        # for consistency and a domain id of 0 (the deploy DOMAIN_ID default —
        # the typed path's runtime domain is the compile-time NROS_ENTRY_DOMAIN_ID).
        set(NROS_ENTRY_APP_DOMAIN_ID 0)
        # Per-image IP last octet (default .10; distinct per test-pair member —
        # identical IP+MAC seeds give identical zenoh ZIDs → one-peer collapse).
        if(NOT DEFINED NROS_ENTRY_IP_LAST)
            set(NROS_ENTRY_IP_LAST 10)
        endif()
        set(_appcfg_src "${_entry_dir}/nros_app_config_def.c")
        configure_file(
            "${_NROS_NODE_REGISTER_DIR}/templates/freertos_app_config.c.in"
            "${_appcfg_src}" @ONLY)

        add_executable(${PROJECT_NAME} "${_entry_src}" "${_appcfg_src}" ${_NRC_SOURCES})
        target_include_directories(${PROJECT_NAME} PRIVATE
            "${CMAKE_CURRENT_SOURCE_DIR}/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/src")
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
        # Issue 0088 — the carrier executable compiles ${_NRC_SOURCES} (C/C++ TUs
        # that include <nros/nros_config_generated.h>); order them after the header
        # mirror targets so they never pick up the in-tree stub.
        _nros_node_register_config_header_deps(${PROJECT_NAME})
        # Issue 0090 — `add_dependencies` (above) orders the TARGET but does not stop
        # a TU compiling before the per-build header is mirrored on the freertos link
        # path, so it reads the in-tree stub (`*_OPAQUE_U64S undeclared`) — exactly the
        # 0088/0090 race the zephyr + threadx carriers already guard. Add a HARD
        # file-level OBJECT_DEPENDS on the Corrosion mirror header(s) so each TU waits
        # for the generated header to exist. Apply to the WHOLE carrier source set: the
        # generated C++ entry (`${_entry_src}`) drives the full nros-cpp surface and the
        # C node source(s) (`${_NRC_SOURCES}`) include <nros/nros_config_generated.h>.
        # The source-file property is directory-scoped, so listing `${_NRC_SOURCES}`
        # ALSO covers the SEPARATE build-coverage component lib (`${_lib}`) that
        # compiles the same file. C carrier reads the C header; C++ reads both — list
        # whichever the build provides (harmless extra edge).
        get_property(_nros_c_cfg_hdr GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)
        get_property(_nros_cpp_cfg_hdr GLOBAL PROPERTY NROS_CPP_CONFIG_HEADER_FILE)
        set(_nros_cfg_hdrs "")
        if(_nros_c_cfg_hdr)
            list(APPEND _nros_cfg_hdrs "${_nros_c_cfg_hdr}")
        endif()
        if(_nros_cpp_cfg_hdr)
            list(APPEND _nros_cfg_hdrs "${_nros_cpp_cfg_hdr}")
        endif()
        if(_nros_cfg_hdrs)
            set_source_files_properties("${_entry_src}" "${_appcfg_src}" ${_NRC_SOURCES}
                PROPERTIES OBJECT_DEPENDS "${_nros_cfg_hdrs}")
        endif()
        nros_platform_link_app(${PROJECT_NAME})
    endif()

    # Phase 244.C4 (RFC-0043) — native (POSIX/host) typed-entry carrier. Mirrors
    # the FreeRTOS self-executable branch above (add_executable + the generated
    # entry + the component sources + nros_platform_link_app), but the host board
    # resolves locator/domain from $NROS_LOCATOR / $ROS_DOMAIN_ID at runtime
    # (`NativeBoard::run_components` -> `nros::init()`), so there is no baked
    # locator and no FreeRTOS app-config TU.
    #
    # TYPED gates the branch (not a FATAL): native supports BOTH the typed carrier
    # AND the imperative hand-written `main` via `nano_ros_entry`. A non-TYPED
    # posix node pkg (declarative / Component-only, e.g. a workspace node compiled
    # only into its component lib above) must fall through here — FATALing would
    # break every non-TYPED posix `nano_ros_node_register` (the 244.C4-collision
    # the phase-247 template sweep hit).
    if((_nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "C")
       AND NANO_ROS_PLATFORM STREQUAL "posix"
       AND _NRC_TYPED
       AND COMMAND nros_platform_link_app
       AND NOT TARGET ${PROJECT_NAME})
        string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")
        set(NROS_ENTRY_PKG_SYM "${_pkg_sym}")
        set(NROS_ENTRY_NODE_NAME "${_NRC_NAME}")
        set(NROS_ENTRY_SHAPE_RCLCPP "${_nrc_shape_rclcpp}")
        set(_entry_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-entry")
        set(_entry_src "${_entry_dir}/main.cpp")
        # `CMAKE_CURRENT_FUNCTION_LIST_DIR` (CMake ≥3.17) resolves to THIS module's
        # dir regardless of include context — unlike the captured
        # `_NROS_NODE_REGISTER_DIR`, which is empty when the module is reached
        # through a workspace add_subdirectory chain (the 244.C4 workspace-subdir
        # bug: `configure_file` resolved a bogus `/templates/...` root path).
        if(_nrc_lang STREQUAL "CPP")
            set(NROS_ENTRY_CLASS "${_NRC_CLASS}")
            set(NROS_ENTRY_CLASS_HEADER "${_nrc_header}")
            configure_file(
                "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/templates/native_entry_main_typed.cpp.in"
                "${_entry_src}" @ONLY)
        else() # C
            configure_file(
                "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/templates/native_entry_main_c_typed.cpp.in"
                "${_entry_src}" @ONLY)
        endif()

        add_executable(${PROJECT_NAME} "${_entry_src}" ${_NRC_SOURCES})
        target_include_directories(${PROJECT_NAME} PRIVATE
            "${CMAKE_CURRENT_SOURCE_DIR}/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/src")
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
        # Issue 0088 — the carrier executable compiles ${_NRC_SOURCES} (C/C++ TUs
        # that include <nros/nros_config_generated.h>); order them after the header
        # mirror targets so they never pick up the in-tree stub.
        _nros_node_register_config_header_deps(${PROJECT_NAME})
        nros_platform_link_app(${PROJECT_NAME})
    endif()

    # Phase 240.8 (RFC-0043) — Zephyr typed-entry carrier. Unlike NuttX (a
    # standalone bootable ELF via add_executable + nros_platform_link_app), a
    # Zephyr app IS the find_package(Zephyr)-owned monolithic `app` target. The
    # carrier APPENDS the generated typed entry TU to `app` and links the
    # component lib (`${_lib}`, built above) into it — no second executable, no
    # per-node component lib the build has to expose separately. The component
    # lib's PUBLIC include dirs (the class header + generated interface libs)
    # propagate to `app`, so the entry TU's `#include "<class_header>"` resolves.
    #
    # The L.4 rule (CLASS starts with `${PROJECT_NAME}::`) means each Node pkg is
    # its own `project(<pkg>)` subdirectory (e.g. ASI `add_subdirectory(controller_pkg)`
    # with `project(controller_pkg)` → CLASS `controller_pkg::Controller`); the
    # Zephyr `app` target is global, so `target_sources(app …)` from that subdir
    # composes into the outer app. SINGLE-NODE per app: one Node pkg deploys to
    # zephyr per `app` (it owns the one `int main`). Multi-node Zephyr uses the
    # `nros codegen entry --typed` multi-node emitter (one entry constructs all
    # nodes) — out of scope here.
    if((_nrc_lang STREQUAL "CPP" OR _nrc_lang STREQUAL "C")
       AND "zephyr" IN_LIST _NRC_DEPLOY
       AND NANO_ROS_PLATFORM STREQUAL "zephyr"
       AND TARGET app
       AND NOT TARGET ${PROJECT_NAME}_nros_zephyr_entry)
        if(NOT _NRC_TYPED)
            message(FATAL_ERROR
                "nano_ros_node_register: the Zephyr carrier requires TYPED — "
                "the RFC-0043 real-callback component path. The legacy "
                "declarative-register entry is not generated on Zephyr.")
        endif()
        set(NROS_ENTRY_NODE_NAME "${_NRC_NAME}")
        set(_zephyr_entry_src "${CMAKE_CURRENT_BINARY_DIR}/nros-entry/zephyr_entry_main.cpp")
        if(_nrc_lang STREQUAL "CPP")
            set(NROS_ENTRY_CLASS "${_NRC_CLASS}")
            set(NROS_ENTRY_CLASS_HEADER "${_nrc_header}")
            set(NROS_ENTRY_SHAPE_RCLCPP "${_nrc_shape_rclcpp}")
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/zephyr_entry_main_typed.cpp.in"
                "${_zephyr_entry_src}" @ONLY)
        else()
            # Phase 244.C2 — Zephyr C typed carrier (mirrors the NuttX C path).
            # The entry TU is C++ but constructs the C component via its
            # `__nros_c_component_<pkg>_*` factory/configure seam
            # (NROS_C_COMPONENT); `NROS_ENTRY_PKG_SYM` is the sanitized pkg name
            # the C source was compiled with.
            string(REGEX REPLACE "[^A-Za-z0-9_]" "_" _pkg_sym "${PROJECT_NAME}")
            set(NROS_ENTRY_PKG_SYM "${_pkg_sym}")
            configure_file(
                "${_NROS_NODE_REGISTER_DIR}/templates/zephyr_entry_main_c_typed.cpp.in"
                "${_zephyr_entry_src}" @ONLY)
        endif()
        # Idempotency marker — guard one entry TU per Node pkg (re-runnable
        # configure). PROJECT_NAME is the Node pkg (its own project()), so the
        # marker is per-pkg, not per-app.
        add_custom_target(${PROJECT_NAME}_nros_zephyr_entry)
        target_sources(app PRIVATE "${_zephyr_entry_src}")
        target_link_libraries(app PRIVATE ${_lib})
    endif()

    _nros_json_strlist(_sources_json ${_NRC_SOURCES})
    _nros_json_strlist(_deploy_json  ${_NRC_DEPLOY})
    _nros_json_strlist(_cbgs_json    ${_NRC_CALLBACK_GROUPS})
    get_property(_acc GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    {\"name\": \"${_NRC_NAME}\", \"class\": \"${_NRC_CLASS}\", \
\"class_header\": \"${_nrc_header}\", \"shape\": \"${_nrc_shape}\", \
\"sources\": [${_sources_json}], \"deploy\": [${_deploy_json}], \
\"pkg_dir\": \"${CMAKE_CURRENT_SOURCE_DIR}\", \"lang\": \"${_nrc_lang_lc}\", \
\"callback_groups\": [${_cbgs_json}]}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_COMPONENTS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()

# (The 212.N.6 `nano_ros_application` and 213.B.1 `nano_ros_component_register`
# deprecation shims were retired in 287-W8 — both caller sweeps completed long
# ago; zero callers remained.)

# (`nano_ros_deploy` was retired post-287 — nothing consumed its
# deploy_targets JSON: the CLI's MetadataDoc reads only `components`, and the
# per-package deploy/rmw tuple lives in package.xml `<export><nano_ros …/>`
# since 287-W4. `nros_system_generate`'s self-pkg detection now keys on the
# package.xml tuple instead of the retired cmake call.)

# Phase 212.N.6 — pull in `nano_ros_entry`. The Entry module
# back-includes this file (guarded) for the shared helpers
# (`_nros_metadata_emit`, `_nros_json_strlist`) + GLOBAL property
# definitions; doing the include LAST ensures those helpers are
# already defined by the time NanoRosEntry's body runs, and that the
# deprecation shim above can resolve `nano_ros_entry` at call time.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosEntry.cmake")
